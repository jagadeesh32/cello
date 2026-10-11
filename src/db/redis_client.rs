//! Native async Redis client exposed to Python.
//!
//! Backed by the `redis` crate's `aio::ConnectionManager` (a multiplexed,
//! auto-reconnecting connection). Commands return Python awaitables via
//! `crate::async_bridge`: the round trip runs on the `cello-io` thread without
//! the GIL and the reply is converted on the event-loop thread.
//!
//! ```python
//! await request.redis.set("k", "v", ex=60)
//! v = await request.redis.get("k")
//! await request.redis.incr("counter")
//! sha = await request.redis.script_load("return 1")
//! await request.redis.evalsha(sha, 0)
//! ```

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use redis::aio::ConnectionManager;
use tokio::sync::Mutex;

use crate::async_bridge as bridge;

fn rt_err(msg: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(msg.to_string())
}

/// Convert a Python value into binary-safe Redis argument bytes.
fn py_to_redis_bytes(obj: &PyAny) -> PyResult<Vec<u8>> {
    if let Ok(b) = obj.downcast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s.into_bytes());
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(i.to_string().into_bytes());
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(f.to_string().into_bytes());
    }
    // dict / list / other → JSON.
    let json_mod = obj.py().import("json")?;
    let dumped: String = json_mod.call_method1("dumps", (obj,))?.extract()?;
    Ok(dumped.into_bytes())
}

/// Convert a `redis::Value` into a native Python object.
fn redis_value_to_py(py: Python<'_>, value: &redis::Value) -> PyObject {
    match value {
        redis::Value::Nil => py.None(),
        redis::Value::Int(i) => i.into_py(py),
        redis::Value::Data(bytes) => match std::str::from_utf8(bytes) {
            Ok(s) => s.into_py(py),
            Err(_) => PyBytes::new(py, bytes).into_py(py),
        },
        redis::Value::Bulk(items) => {
            let list = pyo3::types::PyList::empty(py);
            for item in items {
                let _ = list.append(redis_value_to_py(py, item));
            }
            list.into_py(py)
        }
        redis::Value::Status(s) => s.into_py(py),
        redis::Value::Okay => "OK".into_py(py),
    }
}

/// Native async Redis client.
#[pyclass(name = "Redis")]
#[derive(Clone)]
pub struct PyRedis {
    client: redis::Client,
    manager: Arc<Mutex<Option<ConnectionManager>>>,
    #[pyo3(get)]
    url: String,
}

impl PyRedis {
    /// Build a client from a Redis URL. The connection is opened lazily on first
    /// command, so this is safe to call at configuration time.
    ///
    /// Both `redis://` (plaintext) and `rediss://` (TLS, via rustls with webpki
    /// roots) URLs are supported.
    pub fn connect(url: &str) -> PyResult<Self> {
        let client =
            redis::Client::open(url).map_err(|e| rt_err(format!("invalid Redis URL: {e}")))?;
        Ok(Self {
            client,
            manager: Arc::new(Mutex::new(None)),
            url: url.to_string(),
        })
    }
}

/// Acquire (lazily initialising) a cloned connection manager.
async fn get_conn(
    client: redis::Client,
    manager: Arc<Mutex<Option<ConnectionManager>>>,
) -> PyResult<ConnectionManager> {
    let mut guard = manager.lock().await;
    if guard.is_none() {
        let mgr = client
            .get_connection_manager()
            .await
            .map_err(|e| rt_err(format!("Redis connection failed: {e}")))?;
        *guard = Some(mgr);
    }
    Ok(guard.as_ref().unwrap().clone())
}

impl PyRedis {
    /// Run a prepared `redis::Cmd` and convert the reply to Python.
    fn run<'py>(&self, py: Python<'py>, cmd: redis::Cmd) -> PyResult<&'py PyAny> {
        let client = self.client.clone();
        let manager = self.manager.clone();
        bridge::future_into_py_with(py, async move {
            let mut conn = get_conn(client, manager).await?;
            let value: redis::Value = cmd
                .query_async(&mut conn)
                .await
                .map_err(|e| rt_err(format!("Redis command failed: {e}")))?;
            Ok(move |py: Python<'_>| Ok(redis_value_to_py(py, &value)))
        })
    }
}

#[pymethods]
impl PyRedis {
    // ── Strings / keys ────────────────────────────────────────────────────────

    fn get<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("GET").arg(key).to_owned())
    }

    #[pyo3(signature = (key, value, ex=None, px=None, nx=false, xx=false))]
    #[allow(clippy::too_many_arguments)] // mirrors Redis SET options
    fn set<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: &PyAny,
        ex: Option<i64>,
        px: Option<i64>,
        nx: bool,
        xx: bool,
    ) -> PyResult<&'py PyAny> {
        let val = py_to_redis_bytes(value)?;
        let mut cmd = redis::cmd("SET");
        cmd.arg(key).arg(val);
        if let Some(ex) = ex {
            cmd.arg("EX").arg(ex);
        }
        if let Some(px) = px {
            cmd.arg("PX").arg(px);
        }
        if nx {
            cmd.arg("NX");
        }
        if xx {
            cmd.arg("XX");
        }
        self.run(py, cmd)
    }

    fn setex<'py>(
        &self,
        py: Python<'py>,
        key: String,
        seconds: i64,
        value: &PyAny,
    ) -> PyResult<&'py PyAny> {
        let val = py_to_redis_bytes(value)?;
        self.run(
            py,
            redis::cmd("SETEX")
                .arg(key)
                .arg(seconds)
                .arg(val)
                .to_owned(),
        )
    }

    #[pyo3(signature = (*keys))]
    fn delete<'py>(&self, py: Python<'py>, keys: &PyTuple) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("DEL");
        for k in keys.iter() {
            cmd.arg(k.extract::<String>()?);
        }
        self.run(py, cmd)
    }

    #[pyo3(signature = (*keys))]
    fn exists<'py>(&self, py: Python<'py>, keys: &PyTuple) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("EXISTS");
        for k in keys.iter() {
            cmd.arg(k.extract::<String>()?);
        }
        self.run(py, cmd)
    }

    fn expire<'py>(&self, py: Python<'py>, key: String, seconds: i64) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("EXPIRE").arg(key).arg(seconds).to_owned())
    }

    fn ttl<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("TTL").arg(key).to_owned())
    }

    fn incr<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("INCR").arg(key).to_owned())
    }

    fn incrby<'py>(&self, py: Python<'py>, key: String, amount: i64) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("INCRBY").arg(key).arg(amount).to_owned())
    }

    fn decr<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("DECR").arg(key).to_owned())
    }

    fn decrby<'py>(&self, py: Python<'py>, key: String, amount: i64) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("DECRBY").arg(key).arg(amount).to_owned())
    }

    #[pyo3(signature = (*keys))]
    fn mget<'py>(&self, py: Python<'py>, keys: &PyTuple) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("MGET");
        for k in keys.iter() {
            cmd.arg(k.extract::<String>()?);
        }
        self.run(py, cmd)
    }

    fn keys<'py>(&self, py: Python<'py>, pattern: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("KEYS").arg(pattern).to_owned())
    }

    // ── Hashes ────────────────────────────────────────────────────────────────

    fn hget<'py>(&self, py: Python<'py>, key: String, field: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("HGET").arg(key).arg(field).to_owned())
    }

    fn hset<'py>(
        &self,
        py: Python<'py>,
        key: String,
        field: String,
        value: &PyAny,
    ) -> PyResult<&'py PyAny> {
        let val = py_to_redis_bytes(value)?;
        self.run(
            py,
            redis::cmd("HSET").arg(key).arg(field).arg(val).to_owned(),
        )
    }

    fn hgetall<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("HGETALL").arg(key).to_owned())
    }

    fn hdel<'py>(&self, py: Python<'py>, key: String, field: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("HDEL").arg(key).arg(field).to_owned())
    }

    // ── Lists ─────────────────────────────────────────────────────────────────

    #[pyo3(signature = (key, *values))]
    fn lpush<'py>(&self, py: Python<'py>, key: String, values: &PyTuple) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("LPUSH");
        cmd.arg(key);
        for v in values.iter() {
            cmd.arg(py_to_redis_bytes(v)?);
        }
        self.run(py, cmd)
    }

    #[pyo3(signature = (key, *values))]
    fn rpush<'py>(&self, py: Python<'py>, key: String, values: &PyTuple) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("RPUSH");
        cmd.arg(key);
        for v in values.iter() {
            cmd.arg(py_to_redis_bytes(v)?);
        }
        self.run(py, cmd)
    }

    fn lpop<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("LPOP").arg(key).to_owned())
    }

    fn rpop<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("RPOP").arg(key).to_owned())
    }

    #[pyo3(signature = (key, start=0, stop=-1))]
    fn lrange<'py>(
        &self,
        py: Python<'py>,
        key: String,
        start: i64,
        stop: i64,
    ) -> PyResult<&'py PyAny> {
        self.run(
            py,
            redis::cmd("LRANGE")
                .arg(key)
                .arg(start)
                .arg(stop)
                .to_owned(),
        )
    }

    fn llen<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("LLEN").arg(key).to_owned())
    }

    // ── Sets ──────────────────────────────────────────────────────────────────

    #[pyo3(signature = (key, *members))]
    fn sadd<'py>(&self, py: Python<'py>, key: String, members: &PyTuple) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("SADD");
        cmd.arg(key);
        for m in members.iter() {
            cmd.arg(py_to_redis_bytes(m)?);
        }
        self.run(py, cmd)
    }

    fn srem<'py>(&self, py: Python<'py>, key: String, member: &PyAny) -> PyResult<&'py PyAny> {
        let m = py_to_redis_bytes(member)?;
        self.run(py, redis::cmd("SREM").arg(key).arg(m).to_owned())
    }

    fn smembers<'py>(&self, py: Python<'py>, key: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("SMEMBERS").arg(key).to_owned())
    }

    fn sismember<'py>(&self, py: Python<'py>, key: String, member: &PyAny) -> PyResult<&'py PyAny> {
        let m = py_to_redis_bytes(member)?;
        self.run(py, redis::cmd("SISMEMBER").arg(key).arg(m).to_owned())
    }

    // ── Pub/Sub (publish only), scripting, admin ──────────────────────────────

    fn publish<'py>(
        &self,
        py: Python<'py>,
        channel: String,
        message: &PyAny,
    ) -> PyResult<&'py PyAny> {
        let msg = py_to_redis_bytes(message)?;
        self.run(py, redis::cmd("PUBLISH").arg(channel).arg(msg).to_owned())
    }

    fn script_load<'py>(&self, py: Python<'py>, script: String) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("SCRIPT").arg("LOAD").arg(script).to_owned())
    }

    #[pyo3(signature = (script, numkeys, *keys_and_args))]
    fn eval<'py>(
        &self,
        py: Python<'py>,
        script: String,
        numkeys: i64,
        keys_and_args: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("EVAL");
        cmd.arg(script).arg(numkeys);
        for a in keys_and_args.iter() {
            cmd.arg(py_to_redis_bytes(a)?);
        }
        self.run(py, cmd)
    }

    #[pyo3(signature = (sha, numkeys, *keys_and_args))]
    fn evalsha<'py>(
        &self,
        py: Python<'py>,
        sha: String,
        numkeys: i64,
        keys_and_args: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let mut cmd = redis::cmd("EVALSHA");
        cmd.arg(sha).arg(numkeys);
        for a in keys_and_args.iter() {
            cmd.arg(py_to_redis_bytes(a)?);
        }
        self.run(py, cmd)
    }

    fn ping<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("PING").to_owned())
    }

    fn dbsize<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("DBSIZE").to_owned())
    }

    fn flushdb<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        self.run(py, redis::cmd("FLUSHDB").to_owned())
    }

    /// Close the underlying connection (drops the manager; a later command
    /// transparently reconnects).
    fn close<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let manager = self.manager.clone();
        bridge::future_into_py(py, async move {
            *manager.lock().await = None;
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        format!("<Redis url={}>", self.url)
    }
}
