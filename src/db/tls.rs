//! TLS for native Postgres connections (pool + `cello migrate` session).
//!
//! Implements libpq-style `sslmode` on top of the project's single rustls
//! stack (the same `rustls 0.22` / `tokio-rustls 0.25` the HTTP server and the
//! `rediss://` Redis client use) — no OpenSSL / native-tls.
//!
//! | `sslmode`     | TLS                       | certificate chain | hostname |
//! |---------------|---------------------------|-------------------|----------|
//! | `disable`     | never                     | –                 | –        |
//! | `allow`       | treated as `prefer`       |                   |          |
//! | `prefer`      | if the server offers it   | not verified      | no       |
//! | `require`     | always                    | not verified *    | no       |
//! | `verify-ca`   | always                    | verified          | no       |
//! | `verify-full` | always                    | verified          | yes      |
//!
//! \* As in libpq, `require` with an explicit `sslrootcert` behaves like
//! `verify-ca`.
//!
//! The default when `sslmode` is unspecified is `prefer` (libpq's default).
//! `prefer`/`require` encrypt the traffic but cannot stop an active
//! man-in-the-middle; production deployments should use `verify-full`.
//!
//! Trust roots: `sslrootcert=<PEM path>` trusts exactly the CA(s) in that file
//! (libpq semantics); otherwise — or with `sslrootcert=system` — the bundled
//! Mozilla roots (`webpki-roots`) are used. Client certificates come from
//! `sslcert` + `sslkey` (PEM). Unix-socket hosts never use TLS (the server does
//! not support it there).
//!
//! The ssl* keys are accepted both in the DSN (URL query string or libpq
//! `key=value` form) and as `DatabaseConfig` options; they are stripped from the
//! DSN before it is handed to `tokio_postgres::Config`, which only understands
//! `disable`/`prefer`/`require`.
//!
//! SCRAM channel binding (`tls-server-end-point`, RFC 5929) is supported, so
//! password authentication over TLS uses `SCRAM-SHA-256-PLUS` when the server
//! offers it — this also defeats a MITM in the non-verifying modes when the
//! server uses SCRAM.

use std::fmt;
use std::fs::File;
use std::future::Future;
use std::io::{self, BufReader};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::task::{Context, Poll};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use sha2::{Digest, Sha256, Sha384, Sha512};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_postgres::config::{Host, SslMode as PgSslMode};
use tokio_postgres::tls::{ChannelBinding, MakeTlsConnect, TlsConnect, TlsStream};

/// libpq `sslmode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl FromStr for SslMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "disable" => Ok(Self::Disable),
            // `allow` (plaintext first, TLS on retry) is upgraded to `prefer`:
            // same compatibility, never less secure.
            "allow" | "prefer" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            "verify-ca" | "verify_ca" => Ok(Self::VerifyCa),
            "verify-full" | "verify_full" => Ok(Self::VerifyFull),
            other => Err(format!(
                "invalid sslmode {other:?}: expected one of disable, prefer, require, \
                 verify-ca, verify-full"
            )),
        }
    }
}

impl SslMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }
}

/// The TLS-related connection options (`sslmode`, `sslrootcert`, `sslcert`,
/// `sslkey`). `None` means "not specified".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsOptions {
    pub sslmode: Option<String>,
    pub sslrootcert: Option<String>,
    pub sslcert: Option<String>,
    pub sslkey: Option<String>,
}

impl TlsOptions {
    /// Fill unset fields from `other` (`self` wins).
    pub fn or(self, other: TlsOptions) -> TlsOptions {
        TlsOptions {
            sslmode: self.sslmode.or(other.sslmode),
            sslrootcert: self.sslrootcert.or(other.sslrootcert),
            sslcert: self.sslcert.or(other.sslcert),
            sslkey: self.sslkey.or(other.sslkey),
        }
    }

    fn set(&mut self, key: &str, value: String) -> bool {
        let slot = match key {
            "sslmode" => &mut self.sslmode,
            "sslrootcert" => &mut self.sslrootcert,
            "sslcert" => &mut self.sslcert,
            "sslkey" => &mut self.sslkey,
            _ => return false,
        };
        *slot = Some(value);
        true
    }
}

fn is_tls_key(key: &str) -> bool {
    matches!(key, "sslmode" | "sslrootcert" | "sslcert" | "sslkey")
}

/// Remove the ssl* keys from a DSN, returning the cleaned DSN and the values.
pub fn split_dsn(dsn: &str) -> Result<(String, TlsOptions), String> {
    let trimmed = dsn.trim();
    if trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://") {
        split_url(trimmed)
    } else {
        split_key_value(trimmed)
    }
}

fn split_url(dsn: &str) -> Result<(String, TlsOptions), String> {
    let mut opts = TlsOptions::default();
    let Some((base, query)) = dsn.split_once('?') else {
        return Ok((dsn.to_string(), opts));
    };
    let mut kept: Vec<&str> = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k).map_err(|e| format!("invalid DSN: {e}"))?;
        if is_tls_key(&key) {
            let value = urlencoding::decode(v).map_err(|e| format!("invalid DSN: {e}"))?;
            opts.set(&key, value.into_owned());
        } else {
            kept.push(pair);
        }
    }
    let cleaned = if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    };
    Ok((cleaned, opts))
}

/// libpq `key=value` connection strings: values may be single-quoted, and `\`
/// escapes the next character.
fn split_key_value(dsn: &str) -> Result<(String, TlsOptions), String> {
    let mut opts = TlsOptions::default();
    let mut kept: Vec<&str> = Vec::new();
    let bytes = dsn.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = &dsn[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            return Err(format!("invalid DSN: missing \"=\" after {key:?}"));
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = Vec::new();
        if i < bytes.len() && bytes[i] == b'\'' {
            i += 1;
            loop {
                match bytes.get(i) {
                    None => return Err("invalid DSN: unterminated quoted value".into()),
                    Some(b'\'') => {
                        i += 1;
                        break;
                    }
                    Some(b'\\') if i + 1 < bytes.len() => {
                        value.push(bytes[i + 1]);
                        i += 2;
                    }
                    Some(&c) => {
                        value.push(c);
                        i += 1;
                    }
                }
            }
        } else {
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    value.push(bytes[i + 1]);
                    i += 2;
                } else {
                    value.push(bytes[i]);
                    i += 1;
                }
            }
        }
        if is_tls_key(key) {
            let v = String::from_utf8(value).map_err(|_| "invalid DSN: not UTF-8".to_string())?;
            opts.set(key, v);
        } else {
            kept.push(&dsn[start..i]);
        }
    }
    Ok((kept.join(" "), opts))
}

/// Everything needed to open Postgres connections with the requested TLS
/// behaviour: the driver config (with its `ssl_mode` set) and the connector.
pub struct PgTls {
    pub config: tokio_postgres::Config,
    pub connector: RustlsConnect,
    pub mode: SslMode,
}

/// Parse `dsn` (plus `overrides`, which win over DSN values), load any
/// certificate files, and build the rustls connector. All failures — bad
/// sslmode, unreadable / invalid PEM — are returned as messages, never panics.
pub fn configure(dsn: &str, overrides: TlsOptions) -> Result<PgTls, String> {
    let (clean, from_dsn) = split_dsn(dsn)?;
    let opts = overrides.or(from_dsn);
    let mut config = tokio_postgres::Config::from_str(&clean)
        .map_err(|e| format!("invalid Postgres URL: {e}"))?;

    let mut mode = match &opts.sslmode {
        Some(s) => s.parse::<SslMode>()?,
        None => SslMode::Prefer,
    };
    let rootcert = opts
        .sslrootcert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    // libpq: `require` with an explicit root certificate verifies the chain.
    if mode == SslMode::Require && rootcert.is_some_and(|r| r != "system") {
        mode = SslMode::VerifyCa;
    }

    // The server never offers TLS on a Unix socket; libpq ignores sslmode there.
    let hosts = config.get_hosts();
    let all_unix = !hosts.is_empty() && hosts.iter().all(|h| !matches!(h, Host::Tcp(_)));
    if all_unix {
        mode = SslMode::Disable;
    }

    config.ssl_mode(match mode {
        SslMode::Disable => PgSslMode::Disable,
        SslMode::Prefer => PgSslMode::Prefer,
        _ => PgSslMode::Require,
    });

    let connector = if mode == SslMode::Disable {
        RustlsConnect::disabled()
    } else {
        RustlsConnect::new(build_client_config(mode, rootcert, &opts)?)
    };
    Ok(PgTls {
        config,
        connector,
        mode,
    })
}

fn build_client_config(
    mode: SslMode,
    rootcert: Option<&str>,
    opts: &TlsOptions,
) -> Result<ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let algs = provider.signature_verification_algorithms;
    let verifier: Arc<dyn ServerCertVerifier> = match mode {
        SslMode::Prefer | SslMode::Require => Arc::new(NoVerify { algs }),
        SslMode::VerifyCa | SslMode::VerifyFull => {
            let roots = Arc::new(load_roots(rootcert)?);
            let webpki = WebPkiServerVerifier::builder_with_provider(roots, provider.clone())
                .build()
                .map_err(|e| format!("could not build certificate verifier: {e}"))?;
            if mode == SslMode::VerifyFull {
                webpki
            } else {
                Arc::new(VerifyCaOnly { inner: webpki })
            }
        }
        SslMode::Disable => unreachable!("no TLS config is built for sslmode=disable"),
    };
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS configuration error: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let cert = opts.sslcert.as_deref().filter(|s| !s.is_empty());
    let key = opts.sslkey.as_deref().filter(|s| !s.is_empty());
    match (cert, key) {
        (None, None) => Ok(builder.with_no_client_auth()),
        (Some(cert), Some(key)) => {
            let chain = load_certs(cert, "sslcert")?;
            let key = load_key(key)?;
            builder
                .with_client_auth_cert(chain, key)
                .map_err(|e| format!("sslcert/sslkey rejected: {e}"))
        }
        (Some(_), None) => Err("sslcert was given without sslkey".into()),
        (None, Some(_)) => Err("sslkey was given without sslcert".into()),
    }
}

fn open(path: &str, what: &str) -> Result<BufReader<File>, String> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|e| format!("cannot read {what} {path:?}: {e}"))
}

fn load_certs(path: &str, what: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let mut reader = open(path, what)?;
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("invalid PEM in {what} {path:?}: {e}"))?;
    if certs.is_empty() {
        return Err(format!(
            "{what} {path:?} contains no PEM certificates (expected -----BEGIN CERTIFICATE-----)"
        ));
    }
    Ok(certs)
}

fn load_key(path: &str) -> Result<PrivateKeyDer<'static>, String> {
    let mut reader = open(path, "sslkey")?;
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| format!("invalid PEM in sslkey {path:?}: {e}"))?
        .ok_or_else(|| format!("sslkey {path:?} contains no PEM private key"))
}

fn load_roots(rootcert: Option<&str>) -> Result<RootCertStore, String> {
    let mut roots = RootCertStore::empty();
    match rootcert {
        None | Some("system") => {
            roots
                .roots
                .extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        Some(path) => {
            for cert in load_certs(path, "sslrootcert")? {
                roots
                    .add(cert)
                    .map_err(|e| format!("sslrootcert {path:?}: unusable certificate: {e}"))?;
            }
        }
    }
    Ok(roots)
}

// ---------------------------------------------------------------------------
// Certificate verifiers
// ---------------------------------------------------------------------------

/// `prefer` / `require`: encrypt, but accept any server certificate. The
/// handshake signature is still checked (the server must hold the key for the
/// certificate it presents, which is what channel binding relies on).
#[derive(Debug)]
struct NoVerify {
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// `verify-ca`: full chain / validity / usage verification, but the
/// certificate need not name the host. webpki checks the name last, so a
/// `NotValidForName` error means everything else already passed.
#[derive(Debug)]
struct VerifyCaOnly {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for VerifyCaOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Err(rustls::Error::InvalidCertificate(CertificateError::NotValidForName)) => {
                Ok(ServerCertVerified::assertion())
            }
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

// ---------------------------------------------------------------------------
// tokio-postgres TLS glue
// ---------------------------------------------------------------------------

/// `MakeTlsConnect` over rustls. `None` config = TLS unavailable (used for
/// `sslmode=disable`, where the driver never asks for a handshake).
#[derive(Clone)]
pub struct RustlsConnect {
    config: Option<Arc<ClientConfig>>,
}

impl RustlsConnect {
    fn new(config: ClientConfig) -> Self {
        Self {
            config: Some(Arc::new(config)),
        }
    }

    fn disabled() -> Self {
        Self { config: None }
    }
}

impl fmt::Debug for RustlsConnect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RustlsConnect")
            .field("enabled", &self.config.is_some())
            .finish()
    }
}

impl<S> MakeTlsConnect<S> for RustlsConnect
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = RustlsStream<S>;
    type TlsConnect = RustlsTlsConnect;
    type Error = io::Error;

    /// Called by the driver for every host *before* it knows whether TLS will
    /// be used (also for `sslmode=disable` and Unix sockets), so failures are
    /// deferred to [`TlsConnect::connect`], which only runs for a handshake.
    fn make_tls_connect(&mut self, domain: &str) -> io::Result<RustlsTlsConnect> {
        let target = match self.config.clone() {
            None => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "TLS is disabled (sslmode=disable)",
            )),
            Some(config) => ServerName::try_from(domain.to_string())
                .map(|name| (config, name))
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("invalid TLS server name {domain:?}: {e}"),
                    )
                }),
        };
        Ok(RustlsTlsConnect { target })
    }
}

pub struct RustlsTlsConnect {
    target: io::Result<(Arc<ClientConfig>, ServerName<'static>)>,
}

type ConnectFuture<S> = Pin<Box<dyn Future<Output = io::Result<RustlsStream<S>>> + Send>>;

impl<S> TlsConnect<S> for RustlsTlsConnect
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = RustlsStream<S>;
    type Error = io::Error;
    type Future = ConnectFuture<S>;

    fn connect(self, stream: S) -> ConnectFuture<S> {
        Box::pin(async move {
            let (config, name) = self.target?;
            let tls = tokio_rustls::TlsConnector::from(config)
                .connect(name, stream)
                .await?;
            Ok(RustlsStream(tls))
        })
    }
}

pub struct RustlsStream<S>(tokio_rustls::client::TlsStream<S>);

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for RustlsStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for RustlsStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> TlsStream for RustlsStream<S> {
    fn channel_binding(&self) -> ChannelBinding {
        let (_, conn) = self.0.get_ref();
        conn.peer_certificates()
            .and_then(|certs| certs.first())
            .and_then(|leaf| tls_server_end_point(leaf.as_ref()))
            .map(ChannelBinding::tls_server_end_point)
            .unwrap_or_else(ChannelBinding::none)
    }
}

// ---------------------------------------------------------------------------
// tls-server-end-point channel binding (RFC 5929 §4.1)
// ---------------------------------------------------------------------------

/// Read one DER TLV: `(tag, contents, rest)`.
fn der_tlv(buf: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, buf) = buf.split_first()?;
    let (&first, buf) = buf.split_first()?;
    let (len, buf) = if first < 0x80 {
        (first as usize, buf)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 || buf.len() < n {
            return None;
        }
        let len = buf[..n]
            .iter()
            .fold(0usize, |acc, &b| (acc << 8) | b as usize);
        (len, &buf[n..])
    };
    if buf.len() < len {
        return None;
    }
    Some((tag, &buf[..len], &buf[len..]))
}

#[derive(Debug, PartialEq, Eq)]
enum CbHash {
    Sha256,
    Sha384,
    Sha512,
}

/// The hash for `tls-server-end-point`: the certificate's signature hash, with
/// MD5/SHA-1 upgraded to SHA-256. `None` for algorithms without a single
/// well-defined hash (RSASSA-PSS parameters, Ed25519) — the client then simply
/// does not offer channel binding.
fn cb_hash(cert_der: &[u8]) -> Option<CbHash> {
    const RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01]; // 1.2.840.113549.1.1
    const ECDSA_SHA2: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03]; // 1.2.840.10045.4.3
    const ECDSA_SHA1: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x01]; // 1.2.840.10045.4.1

    let (0x30, cert, _) = der_tlv(cert_der)? else {
        return None;
    };
    let (0x30, _tbs, rest) = der_tlv(cert)? else {
        return None;
    };
    let (0x30, alg, _) = der_tlv(rest)? else {
        return None;
    };
    let (0x06, oid, _) = der_tlv(alg)? else {
        return None;
    };
    if oid == ECDSA_SHA1 {
        return Some(CbHash::Sha256);
    }
    let (prefix, last) = oid.split_at(oid.len().checked_sub(1)?);
    match (prefix, last[0]) {
        // md5 / sha1 / sha256 / sha224 WithRSAEncryption
        (RSA, 0x04 | 0x05 | 0x0b | 0x0e) => Some(CbHash::Sha256),
        (RSA, 0x0c) => Some(CbHash::Sha384),
        (RSA, 0x0d) => Some(CbHash::Sha512),
        (ECDSA_SHA2, 0x01 | 0x02) => Some(CbHash::Sha256),
        (ECDSA_SHA2, 0x03) => Some(CbHash::Sha384),
        (ECDSA_SHA2, 0x04) => Some(CbHash::Sha512),
        _ => None,
    }
}

fn tls_server_end_point(cert_der: &[u8]) -> Option<Vec<u8>> {
    Some(match cb_hash(cert_der)? {
        CbHash::Sha256 => Sha256::digest(cert_der).to_vec(),
        CbHash::Sha384 => Sha384::digest(cert_der).to_vec(),
        CbHash::Sha512 => Sha512::digest(cert_der).to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_strips_ssl_keys() {
        let (dsn, o) = split_dsn(
            "postgresql://u:p@db.example.com:5432/app?sslmode=verify-full&application_name=x\
             &sslrootcert=%2Fetc%2Fca.pem",
        )
        .unwrap();
        assert_eq!(
            dsn,
            "postgresql://u:p@db.example.com:5432/app?application_name=x"
        );
        assert_eq!(o.sslmode.as_deref(), Some("verify-full"));
        assert_eq!(o.sslrootcert.as_deref(), Some("/etc/ca.pem"));
    }

    #[test]
    fn url_without_remaining_query() {
        let (dsn, o) = split_dsn("postgres://h/db?sslmode=require").unwrap();
        assert_eq!(dsn, "postgres://h/db");
        assert_eq!(o.sslmode.as_deref(), Some("require"));
    }

    #[test]
    fn key_value_strips_ssl_keys() {
        let (dsn, o) = split_dsn(
            "host=db user=app sslmode = verify-ca sslrootcert='/a b/ca.pem' password='x\\'y'",
        )
        .unwrap();
        assert_eq!(dsn, "host=db user=app password='x\\'y'");
        assert_eq!(o.sslmode.as_deref(), Some("verify-ca"));
        assert_eq!(o.sslrootcert.as_deref(), Some("/a b/ca.pem"));
    }

    #[test]
    fn overrides_win() {
        let o = TlsOptions {
            sslmode: Some("disable".into()),
            ..Default::default()
        }
        .or(TlsOptions {
            sslmode: Some("require".into()),
            sslrootcert: Some("x".into()),
            ..Default::default()
        });
        assert_eq!(o.sslmode.as_deref(), Some("disable"));
        assert_eq!(o.sslrootcert.as_deref(), Some("x"));
    }

    #[test]
    fn modes() {
        assert_eq!("verify-full".parse::<SslMode>(), Ok(SslMode::VerifyFull));
        assert_eq!("allow".parse::<SslMode>(), Ok(SslMode::Prefer));
        assert!("bogus".parse::<SslMode>().is_err());
    }

    #[test]
    fn default_is_prefer_and_unix_disables() {
        assert_eq!(
            configure("postgres://u@localhost/db", TlsOptions::default())
                .unwrap()
                .mode,
            SslMode::Prefer
        );
        let unix = configure(
            "host=/var/run/postgresql user=u sslmode=verify-full",
            TlsOptions::default(),
        )
        .unwrap();
        assert_eq!(unix.mode, SslMode::Disable);
    }

    #[test]
    fn bad_paths_are_errors() {
        let e = configure(
            "postgres://u@h/db?sslmode=verify-full&sslrootcert=/nonexistent/ca.pem",
            TlsOptions::default(),
        )
        .err()
        .unwrap();
        assert!(e.contains("sslrootcert"), "{e}");
        let e = configure(
            "postgres://u@h/db?sslmode=require&sslcert=/x.pem",
            TlsOptions::default(),
        )
        .err()
        .unwrap();
        assert!(e.contains("without sslkey"), "{e}");
    }

    #[test]
    fn der_hash_selection() {
        // Minimal Certificate SEQUENCE { tbs SEQUENCE {}, alg SEQUENCE { OID } }.
        let mk = |oid: &[u8]| {
            let mut alg = vec![0x06, oid.len() as u8];
            alg.extend_from_slice(oid);
            let mut inner = vec![0x30, 0x00, 0x30, alg.len() as u8];
            inner.extend_from_slice(&alg);
            let mut cert = vec![0x30, inner.len() as u8];
            cert.extend_from_slice(&inner);
            cert
        };
        let sha256_rsa = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
        let sha384_ec = [0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03];
        let sha1_rsa = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x05];
        let ed25519 = [0x2b, 0x65, 0x70];
        assert_eq!(cb_hash(&mk(&sha256_rsa)), Some(CbHash::Sha256));
        assert_eq!(cb_hash(&mk(&sha384_ec)), Some(CbHash::Sha384));
        assert_eq!(cb_hash(&mk(&sha1_rsa)), Some(CbHash::Sha256));
        assert_eq!(cb_hash(&mk(&ed25519)), None);
        assert_eq!(cb_hash(&[0x30]), None);
    }
}
