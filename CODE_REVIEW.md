# Cello 1.4.0 architecture, security, and performance review

Read-only review of the current tree (`src/`, `python/cello/`). No code was changed. Findings below are from the source, with file and line references. This was not a dynamic test run, fuzz pass, or `cargo audit`.

The HTTP core, the ORM identifier rules, and the v1.3–v1.4 hardening are real. The public surface is much larger than the code that actually runs on a request. Several security controls are bypassable with the defaults a typical app will ship.

## Verdict

Treat Cello as a fast HTTP and Postgres/ORM runtime with optional middleware, and treat GraphQL, WebSocket, gRPC, messaging, CQRS, saga, event sourcing, HTTP/2, and HTTP/3 as unfinished. Do not put a default `app.run()` plus `enable_caching()`, `enable_jwt()`, and `enable_graphql()` on the public internet until the high findings below are fixed.

## Findings

| ID | Severity | Area | Issue |
|----|----------|------|--------|
| S1 | High | Auth bypass | `/metrics`, `/health*`, and `/graphql` are served on the route-miss path and never see JWT, sessions, or rate limits |
| S2 | High | Rate limit | Client IP is the attacker-supplied `X-Forwarded-For`. The accepted socket address is discarded. `key_by` is ignored |
| S3 | High | Cache | GET/HEAD responses are cached by method + path + query, including authenticated bodies, and a HIT skips `after` hooks |
| S4 | High | Sessions | Updates to an existing session are never written. `regenerate_on_auth` is never used. One `Set-Cookie` header total |
| S5 | High | CSRF | The token helper writes a form field the middleware never reads. Double-submit is not bound to the session |
| S6 | High | Defaults | `app.run()` defaults to `env="development"`, which returns handler exception text to clients. Windows workers skip the production switch |
| S7 | High | CORS | `allow_credentials()` turns "any origin" into a reflection of the request `Origin` plus `Access-Control-Allow-Credentials: true` |
| B1 | High | Static files | Default hidden pattern `"."` rejects every path that contains a dot, so normal assets 404 |
| B2 | High | Static files | `Stop` in `before` skips security headers. Precompressed siblings are opened without a second root check |
| B3 | Medium | Compression | Gzip replaces the body and leaves a stale `Content-Length` |
| B4 | Medium | Limits | `LimitsConfig.max_header_size`, `max_connections`, and `max_requests_per_connection` are stored and never applied. Header and body timeouts default off |
| B5 | Medium | Auth compare | The Python Basic Auth and API-key paths use `HashMap` lookup and `==` |
| B6 | Medium | Process model | Unix workers are N+1, forked after the Postgres and Redis clients already exist. Windows workers drop CLI host, port, and the error-exposure flag |
| B7 | Medium | WebSocket / protocols | `app.websocket()` registers a handler the server never upgrades. HTTP/2 and HTTP/3 config is not used by the accept loop |
| P1 | High | Availability | Static files, and any other sync filesystem read, run to completion on the current-thread runtime with the whole file in RAM |
| P2 | Medium | Memory | Session, rate-limit, and response-cache maps have no cap and no reaper. A new session is created for every cookieless request |
| P3 | Medium | Hot path | Body and JSON parsing clone the payload. Response headers are a `HashMap`, so duplicate headers cannot exist |

## Security

### S1. Unrouted endpoints skip authentication

Route matching happens first. A miss serves Prometheus, then any middleware that claims the path, and returns. JWT, Basic Auth, API keys, sessions, CSRF, and rate limiting do not implement `serves_unrouted`, so they do not run.

```862:872:src/server/mod.rs
    let route_match = match route_match {
        Some(m) => m,
        None => {
            // `/metrics` is not a registered route, so serve it here before 404.
            {
                let prom_guard = prometheus.read();
                if let Some(ref p) = *prom_guard {
                    if let Some(response) = p.try_serve(path) {
                        return build_hyper_response(&response, metrics);
                    }
                }
            }
```

`HealthCheckMiddleware::serves_unrouted` claims `/health` and `/health/*` (`src/middleware/health.rs` around line 460). `GraphQLMiddleware::serves_unrouted` claims the GraphQL path (`src/middleware/graphql.rs` around line 570). GraphQL defaults to playground and introspection on (`playground: true`, `introspection: true` around lines 148–150), and the parser is a substring check for `__schema` / `__type` plus the first word after `{`. `async-graphql` is an optional dependency and is not referenced from `src/`.

Enabling JWT does not protect these URLs. Health checks also run expensive probes (database, disk, process memory) for any client that can open a TCP connection.

### S2. Rate limiting trusts the client for its own identity

`listener.accept()` yields `peer_addr` and uses it only in a log line (`src/server/mod.rs` around 603–608). It is not stored on the request. Nothing in the tree writes `context["remote_addr"]`.

```671:689:src/middleware/rate_limit.rs
    pub fn client_ip() -> KeyExtractor {
        Arc::new(|request: &Request| {
            if let Some(xff) = request.headers.get("x-forwarded-for") {
                if let Some(ip) = xff.split(',').next() {
                    return ip.trim().to_string();
                }
            }
            // ...
            request
                .context
                .get("remote_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        })
    }
```

A client that sends a new `X-Forwarded-For` on each request gets a new bucket. A client that sends none shares the bucket `"unknown"` with every other such client, so one sender can lock the limiter for everyone. `Request::client_ip` (`src/request/mod.rs` around 291–300) and `Request::is_secure` / `host` have the same trust model for `X-Forwarded-Proto` and `X-Forwarded-Host`.

`PyRateLimitConfig.key_by` defaults to `"ip"` and is never read. `enable_rate_limit` (`src/lib.rs` around 263–298) always installs `KeyExtractors::client_ip()`.

### S3. Response cache mixes users and drops security headers

The default key is method, path, and sorted query (`create_cache_key`, `src/middleware/cache.rs` around 791–806). `include_user` is false, `include_headers` is empty, and `should_cache_request` does not look at `Authorization` or `Cookie`. Status codes 200 through 226 are all cacheable. The stored entry also gets `Cache-Control: max-age=<ttl>`, which is a signal to browsers and shared caches.

A HIT returns `Stop` from async `before` (`src/server/mod.rs` around 1024–1028), which returns the response immediately. Sync `after` hooks do not run, so that response has no security headers and no CORS headers. The copy was taken in async `after`, which runs before sync `after`, so the stored headers never included them either.

There is no maximum entry count. Keys are attacker-influenced through the query string.

### S4. Sessions do not survive the second request

`SessionData.modified` is `#[serde(skip)]` (`src/middleware/session.rs` around 35–37). `before` stores the session as JSON. `after` deserializes it, so `modified` is always false. The save condition is `is_new || data.modified` (around 541). The first response writes an empty new session. Later mutations of that session are dropped.

`regenerate_on_auth` defaults to true and is never read. `regenerate_session` is a free function with no Python wrapper and no login hook. A session id issued before login stays valid after login.

`Response.headers` is a `HashMap<String, String>` (`src/response/mod.rs` around 56 and 361–367). `set_header` replaces. Session (`Set-Cookie` around line 547) and CSRF (`Set-Cookie` around line 488) overwrite each other. On a first GET, both run, and the earlier middleware in the reversed `after` walk loses its cookie. Browsers then never receive both cookies.

The store is an in-process `HashMap`. Expired entries are left in place until something calls `cleanup()`, and nothing schedules that. Workers do not share the map. Every request without a valid cookie inserts a new id.

### S5. CSRF does not protect the form it tells you to render

`csrf_hidden_field` emits `<input name="_csrf">` (`src/middleware/csrf.rs` around 517–523). `get_request_token` reads the header and the query string only (around 356–368). A normal POST body is ignored, so the documented form fails closed. Putting the token in the query string is the path the API leaves open, and that value lands in access logs, `Referer`, and history.

Double-submit compares the cookie to the header and does not bind the token to the session. The cookie is intentionally not `HttpOnly` (around 420–422). There is no `__Host-` prefix. Signed-token mode checks the HMAC and does not require the cookie to match, so a token leaked once is a bearer token until `max_age`.

`TRACE` is in the default safe-method list (around 192–198).

### S6. Error details default on, and Windows workers keep them on

`EXPOSE_ERROR_DETAILS` starts as `true` (`src/server/mod.rs` around 322–323). `App.run` defaults `env` to `"development"` (`python/cello/__init__.py` around 1721 and 1786–1816), and development sets the flag on. An unhandled handler error, including database messages that contain constraint names and row values, is returned as JSON.

Unix fork happens after `set_expose_errors`, so the child inherits the flag. The Windows path re-executes the script with `CELLO_WORKER=1` and returns before `set_expose_errors` (around 1747–1752). Those workers stay at the Rust default of exposed errors, and they also ignore the host and port the parent parsed from the CLI, because that parse happens after the early return.

### S7. Credentialed CORS reflects any origin

`CorsConfig::new` allows every origin and caches preflight for 86400 seconds (`src/middleware/cors.rs` around 100–108). `enable_cors()` with no arguments installs that (`src/lib.rs` around 227–232).

```228:235:src/middleware/cors.rs
    pub fn allow_credentials(mut self) -> Self {
        self.credentials = true;
        if let AllowedOrigins::Any = self.origins {
            self.origins = AllowedOrigins::Mirror;
        }
        self
    }
```

Browsers reject `Access-Control-Allow-Origin: *` together with credentials. Reflecting the request `Origin`, including `Origin: null`, with `Access-Control-Allow-Credentials: true` is the combination browsers will accept. Any site can then make credentialed cross-origin calls.

`domain_matches` (`src/middleware/cors.rs` around 493–498) treats `*.example.com` as `ends_with("example.com")`, so `notexample.com` matches. The middleware does not call this function today. The tests never pass a suffix collision.

### S8. Other authentication notes

These are smaller, and they are still real.

- Python `enable_basic_auth` compares with `creds.get(u).map(|stored| stored == p)` (`src/lib.rs` around 496–498). A missing username skips the password compare. The single-user Rust constructor does use `subtle`. `ApiKeyAuth::from_keys` is `keys.get(key)` (`src/middleware/auth.rs` around 561–566), which is not a constant-time scan of the key set.
- JWT from Python is HMAC-only, `exp` is required, and the algorithm is not taken from the token. That part is sound. `issuer` and `audience` are hardcoded to `None` in `enable_jwt` (`src/lib.rs` around 403–408), so the Rust checks for them are unreachable from Python. Validation failures are returned to the client via `e.to_string()` (`src/middleware/auth.rs` around 316–319).
- Cookie JWT is opt-in and has no CSRF pairing inside the framework. A cookie that the browser attaches is enough to authorize a cross-site request when `SameSite` allows it.
- `with_headers()` on the logger prints every header, including `Authorization` and `Cookie` (`src/middleware/mod.rs` around 599–605).
- Postgres `sslmode` defaults to `prefer`: the connection is encrypted when the server offers TLS, and the certificate is not checked (`src/db/tls.rs` around 10–21). That is documented. It does not stop an active network attacker. Production pools need `verify-full`.

## Bugs

### B1. Static files hide every dotted name

```187:187:src/middleware/static_files.rs
            hidden_patterns: vec![".".to_string(), "..".to_string()],
```

```293:297:src/middleware/static_files.rs
        for pattern in &self.config.hidden_patterns {
            if decoded.contains(pattern.as_str()) || decoded.starts_with(pattern.as_str()) {
                return None;
            }
        }
```

`decoded.contains(".")` is true for `app.js`, `style.css`, and `index.html`. Those requests fall through and 404. The check is a substring match, so it also does not mean "this path segment is a dotfile."

The URL mount is a raw `starts_with` (around 274). A mount of `/static` also claims `/static-other/...`. On Windows the root check uses string `starts_with` after stripping `\\?\` (around 326–338), so `C:\site\static-evil` passes a root of `C:\site\static`. The Unix branch uses `Path::starts_with`, which compares components.

### B2. Static responses skip the rest of the pipeline

`StaticFilesMiddleware::before` returns `Stop` (around 559–561). `execute_before` returns that action immediately, and `handle_request` writes it out. Security headers, CORS, and compression never see the response.

`precompressed` defaults to true. The requested path is canonicalized, then `photo.jpg.gz` is built with `with_extension` and opened with `File::open` (around 384–401 and 466). `File::open` follows a symlink. A symlink planted next to a real asset is read as that asset's precompressed variant, outside the root check.

Directory listing is off by default. When enabled, file names are interpolated into HTML with no escaping (around 532–534). An SVG is served as `image/svg+xml` (around 114) with no `Content-Disposition` and, because of the `Stop` path, no CSP.

The file is read with `read_to_end` on the connection task (around 465–468). There is no size cap beyond what the OS will allocate.

### B3. Compression leaves the old Content-Length

`CompressionMiddleware::after` replaces the body and sets `Content-Encoding` (`src/middleware/mod.rs` around 731–736). It does not delete or rewrite `Content-Length`. `Response::file` and the range helper set `Content-Length` (`src/response/mod.rs` around 200, 227, 265). `build_hyper_response` copies that header onto the compressed body. Clients and proxies that honor `Content-Length` will truncate the gzip stream or wait for bytes that will never arrive. `Accept-Encoding` is a substring test, so `gzip;q=0` still compresses.

### B4. Limit and timeout fields that do not reach the server

`set_limits` copies only `max_body_size` (`src/lib.rs` around 198–199). `max_header_size`, `max_connections`, and `max_requests_per_connection` on `LimitsConfig` have no effect. `set_timeouts` copies three fields and ignores `write_timeout` and `idle_timeout`.

The server default is a 100 MB body, 10_000 connections, and no header-read timeout (`src/server/mod.rs` around 96–102). `max_body_size = 0` means unlimited. Slowloris protection exists only after an explicit `set_timeouts`. The connection-limit check is also a load-then-spawn race (around 605–615): many accepts can pass before `inc_connections`.

The chunked-body cap via `Limited` is in place (around 960–968). That part holds.

### B5. Process model

`_run_multiprocess_fork` forks `workers` children and the parent also calls `_app.run` (`python/cello/__init__.py` around 1897–1923). A request for 4 workers is 5 processes.

`enable_database` and `enable_redis` connect before `run` (`src/lib.rs` around 636–659). The fork then duplicates those sockets. Postgres and Redis clients are not safe across `fork` once a connection exists. Child and parent will write the same TCP session.

The reload watcher walks the whole working tree every second (`_watch_files`, around 1996–2028).

### B6. Advertised protocols that are not on the request path

`Server::serve` builds an `http1::Builder` only. `websocket_handlers` is moved into `_websocket_handlers` and never read (`src/server/mod.rs` around 540). `Cello::websocket` only inserts into that registry (`src/lib.rs` around 181–183). `WebSocket` in `src/websocket.rs` is a queue with a comment that upgrade handling is still required (around 87–90).

`ServerConfig.http2` and `http3` are never consulted by the accept loop. `enable_grpc`, `enable_messaging`, `enable_rabbitmq`, `enable_sqs`, `enable_event_sourcing`, `enable_cqrs`, and `enable_saga` print that they record config only. The Python modules beside them are a second implementation, not something the HTTP server calls.

`sse_event` strips CR/LF from `event` and `id`. `sse_comment` does not (`src/response/streaming.rs` around 452–454), so a comment that contains a blank line can inject an SSE field.

### B7. Smaller correctness issues

- OpenAPI docs are built by interpolating the title and version into a Python source string and `py.eval`-ing it (`src/lib.rs` around 906–924). A title that contains a quote breaks the generated handler. A title taken from untrusted config is code execution at startup.
- `int_to_py` casts `u64` row counts to `i64` (`src/db/postgres.rs` around 75–76). A count above `i64::MAX` wraps negative.
- `Response::from_json_value` uses `unwrap_or_default` on serialization failure and returns an empty 200 (`src/response/mod.rs` around 453–454).
- Duplicate request headers collapse in a `HashMap` (`src/server/mod.rs` around 929–931). The last value wins, including a second `Content-Length` or `Cookie`.
- CSRF signed-token age uses `now - timestamp` on `u64` (`src/middleware/csrf.rs` around 126). Release builds disable overflow checks (`Cargo.toml` profile.release), so a future timestamp wraps and is rejected. A debug build panics, and `panic = "abort"` is the release profile, so any other panic on the request path kills the process.

## Inefficient logic

The fast path in `handle_request` (skip middleware, return `JsonBytes` straight to hyper) is the right shape. It is disabled as soon as any middleware exists, and the middleware that security requires is what disables it. That tradeoff is fine. The costs below are not.

- The runtime is current-thread. A sync handler that blocks is timed and then moved to the blocking pool after two slow calls. Until that promotion, and for every static-file read, the thread that accepts and parses HTTP is stalled. CPU-bound Python still holds the GIL after offload. `handler_timeout` returns 504 and does not free the pool thread. Those limits are already noted in the v1.3 notes. They still bound any production deployment.
- `Request::json` copies the body into a `String`, parses it, stores the value, and clones it out of the cache (`src/request/mod.rs` around 140–159). `body()` and `text()` also clone. The body was already buffered up to `max_body_size` before the handler ran.
- Static serving allocates a `Vec` the size of the file and copies it again into the hyper body. There is no `sendfile` and no range streaming on that path.
- In-memory session, rate-limit, and cache stores grow with attacker-controlled keys. The cache clones the full body into the map and again on each HIT.
- Gzip at level 6 runs on the server thread inside `after`.
- CORS builds `Allow-Methods` by collecting and joining on every preflight, and `add_cors_headers` appends `Origin` to `Vary` even when it already set `Vary: Origin` (around 404–444).
- `quote_ident` allocates a `String` per column on every query. That is cheap next to Postgres, and it is still on the ORM hot path. The v1.4 decision to skip a native query compiler matches the measurement that query building was under 3 percent. Leave that alone until the identity and cache bugs are fixed.
- `overflow-checks = false` in release is a speed flag on a parser that does length and timestamp arithmetic. Turn it back on. The cost is noise next to Python on the request path.

## What is in good shape

Worth keeping, and worth not simplifying away:

- Query identifiers go through the schema registry. Unknown names become `FieldError`. Values are `$1` parameters. `LIKE` wildcards are escaped. `quote_ident` doubles quotes (`src/schema/ident.rs`).
- JWT verification pins the algorithm to the server config and rejects tokens without `exp`. The Python API cannot select `none` or RSA.
- CSRF origin checks compare the full authority, after an earlier suffix bug.
- The single-user Basic Auth path compares both sides with `subtle` and does not short-circuit.
- Request header values have CR, LF, and NUL stripped on the way out.
- Chunked bodies are capped with `Limited`. Declared `Content-Length` above the cap returns 413 before the buffer.
- Production mode on the Unix path replaces handler errors with a generic 500 and logs the detail.
- `write_only` / `read_only`, rejection of NUL in bodies, and ignoring client-supplied primary keys are the right mass-assignment rules.
- Postgres TLS modes are explicit, including `verify-full`.
- MiniJinja auto-escape defaults on for HTML.

## Upgrades, in the order to do them

1. Pass the accept address into the request, and use it for rate limits and `client_ip`. Honor `X-Forwarded-For` only from a configured list of proxy hops. Either honor `key_by` or remove it from the Python config.
2. Run authentication and rate limiting for `/metrics`, `/health`, and `/graphql`, or register those paths as real routes so the normal chain runs. Default the playground and introspection to off.
3. Make the cache key include a caller-supplied identity, skip caching when `Authorization` or `Cookie` is present, and do not emit public `Cache-Control` unless the handler asked. Apply security headers on the HIT path. Cap the store.
4. Persist session mutations. Call id regeneration on login. Store response headers as a multimap so every `Set-Cookie` is sent. Share sessions across workers only through a store that can expire keys.
5. Read the CSRF token from the parsed form body. Bind the double-submit cookie to the session, or use the signed token and stop accepting it from the query string.
6. Default `app.run(env=...)` to production-safe error hiding when the process is not a TTY, and set the flag inside the `CELLO_WORKER` branch. Create database and Redis pools after fork, in each worker. Make the parent a supervisor, not an extra worker.
7. Fix the static-file hidden check to path segments (leading-dot names). Require a `/` boundary on the URL mount. Re-canonicalize precompressed paths. Stream files and cap the size. Run `after` hooks for `Stop` responses, or apply a fixed security-header set on that path.
8. Drop or rewrite `Content-Length` when compressing. Treat `q=0` as "do not encode."
9. Apply `max_header_size`, `max_connections`, and idle timeout, or delete those fields from the public config so callers are not misled. Default a header-read timeout on.
10. Delete or clearly un-export the dead WebSocket, HTTP/2, and HTTP/3 server paths until they are wired. The GraphQL middleware needs a real parser and the auth chain, or it should leave the public `enable_graphql` API.
11. Refresh the stack and run `cargo audit`. The pins that are far behind current majors are PyO3 0.20, reqwest 0.11, rustls 0.22, and the OpenTelemetry 0.21 / 0.14 crates. Re-enable overflow checks in the release profile.
12. Add constant-time equality on the Python Basic Auth and API-key maps, and stop returning JWT library errors to the client.

Do not start with a native query compiler, HTTP/3, or a service mesh. The request path's identity, cache, session, and "this route never sees auth" bugs dominate both the security risk and the wasted work.
