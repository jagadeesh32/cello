//! Rust-native async HTTP client exposed to Python via PyO3.
//!
//! Backed by reqwest + Tokio. The GIL is never held during network I/O —
//! coroutines are driven by `pyo3_asyncio::tokio::future_into_py`, so HTTP
//! wait time is pure Rust with no Python scheduler overhead.

use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::PyBytes;

// ── Response ──────────────────────────────────────────────────────────────────

/// HTTP response returned by `AsyncClient`.
#[pyclass(name = "HttpResponse")]
pub struct PyHttpResponse {
    #[pyo3(get)]
    pub status: u16,
    body: Vec<u8>,
    hdrs: HashMap<String, String>,
}

#[pymethods]
impl PyHttpResponse {
    /// Raw response body as bytes.
    #[getter]
    fn content<'py>(&self, py: Python<'py>) -> &'py PyBytes {
        PyBytes::new(py, &self.body)
    }

    /// Response body decoded as UTF-8 text.
    #[getter]
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Response headers as a plain dict.
    #[getter]
    fn headers(&self) -> HashMap<String, String> {
        self.hdrs.clone()
    }

    /// Parse response body as JSON, returning a Python object.
    fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_mod = py.import("json")?;
        let raw = PyBytes::new(py, &self.body);
        Ok(json_mod.call_method1("loads", (raw,))?.into_py(py))
    }

    fn __repr__(&self) -> String {
        format!("<HttpResponse status={}>", self.status)
    }
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Rust-native async HTTP client.
///
/// Backed by `reqwest` + Tokio. The GIL is released for the entire duration
/// of the network I/O — no thread-pool, no asyncio scheduler overhead.
///
/// Example::
///
///     from cello import App, AsyncClient
///
///     client = AsyncClient()
///
///     @app.get("/proxy")
///     async def proxy(request):
///         resp = await client.get("https://example.com")
///         return {"status": resp.status, "body": resp.text}
#[pyclass(name = "AsyncClient")]
pub struct PyAsyncClient {
    client: reqwest::Client,
}

#[pymethods]
impl PyAsyncClient {
    #[new]
    #[pyo3(signature = (timeout = 30.0))]
    fn new(timeout: f64) -> PyResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs_f64(timeout))
            .build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { client })
    }

    /// Send a GET request.
    #[pyo3(signature = (url, headers = None))]
    fn get<'py>(
        &self,
        py: Python<'py>,
        url: String,
        headers: Option<HashMap<String, String>>,
    ) -> PyResult<&'py PyAny> {
        let client = self.client.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            dispatch(client.get(&url), headers, None, None).await
        })
    }

    /// Send a POST request.
    #[pyo3(signature = (url, json = None, content = None, headers = None))]
    fn post<'py>(
        &self,
        py: Python<'py>,
        url: String,
        json: Option<PyObject>,
        content: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
    ) -> PyResult<&'py PyAny> {
        let json_bytes = to_json_bytes(py, json)?;
        let client = self.client.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            dispatch(client.post(&url), headers, json_bytes, content).await
        })
    }

    /// Send a PUT request.
    #[pyo3(signature = (url, json = None, content = None, headers = None))]
    fn put<'py>(
        &self,
        py: Python<'py>,
        url: String,
        json: Option<PyObject>,
        content: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
    ) -> PyResult<&'py PyAny> {
        let json_bytes = to_json_bytes(py, json)?;
        let client = self.client.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            dispatch(client.put(&url), headers, json_bytes, content).await
        })
    }

    /// Send a PATCH request.
    #[pyo3(signature = (url, json = None, content = None, headers = None))]
    fn patch<'py>(
        &self,
        py: Python<'py>,
        url: String,
        json: Option<PyObject>,
        content: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
    ) -> PyResult<&'py PyAny> {
        let json_bytes = to_json_bytes(py, json)?;
        let client = self.client.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            dispatch(client.patch(&url), headers, json_bytes, content).await
        })
    }

    /// Send a DELETE request.
    #[pyo3(signature = (url, headers = None))]
    fn delete<'py>(
        &self,
        py: Python<'py>,
        url: String,
        headers: Option<HashMap<String, String>>,
    ) -> PyResult<&'py PyAny> {
        let client = self.client.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            dispatch(client.delete(&url), headers, None, None).await
        })
    }

    /// Async context manager support — `async with AsyncClient() as client`.
    fn __aenter__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<&'py PyAny> {
        let py_self = slf.into_py(py);
        pyo3_asyncio::tokio::future_into_py(py, async move { Ok(py_self) })
    }

    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: PyObject,
        _exc_val: PyObject,
        _exc_tb: PyObject,
    ) -> PyResult<&'py PyAny> {
        pyo3_asyncio::tokio::future_into_py(py, async {
            Ok(Python::with_gil(|py| py.None()))
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Serialize a Python object to JSON bytes while the GIL is held.
/// Returns `None` when `json` is `None` (no body).
fn to_json_bytes(py: Python<'_>, json: Option<PyObject>) -> PyResult<Option<Vec<u8>>> {
    match json {
        None => Ok(None),
        Some(obj) => {
            let s = py
                .import("json")?
                .call_method1("dumps", (obj.as_ref(py),))?
                .extract::<String>()?;
            Ok(Some(s.into_bytes()))
        }
    }
}

/// Build and fire the request, then collect the response into a `PyHttpResponse`.
///
/// The GIL is held only for the final `into_py` call; all network I/O runs
/// without the GIL.
async fn dispatch(
    mut builder: reqwest::RequestBuilder,
    headers: Option<HashMap<String, String>>,
    json_bytes: Option<Vec<u8>>,
    content: Option<Vec<u8>>,
) -> PyResult<PyObject> {
    if let Some(hdrs) = headers {
        for (k, v) in hdrs {
            builder = builder.header(k.as_str(), v.as_str());
        }
    }

    if let Some(bytes) = json_bytes {
        builder = builder
            .header("Content-Type", "application/json")
            .body(bytes);
    } else if let Some(body) = content {
        builder = builder.body(body);
    }

    let resp = builder
        .send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let status = resp.status().as_u16();
    let hdrs: HashMap<String, String> = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();
    let body = resp
        .bytes()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?
        .to_vec();

    Python::with_gil(|py| Ok(PyHttpResponse { status, body, hdrs }.into_py(py)))
}
