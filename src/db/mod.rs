//! Native async data layer: real Postgres pool + Redis client.
//!
//! Replaces the previous mock scaffolding (`middleware::database` /
//! `middleware::redis`) with connection pools that actually talk to a server.
//! Both expose Python awaitables through `crate::async_bridge` (shared with the
//! `AsyncClient` in `src/http_client.rs`).

pub mod error;
pub mod postgres;
pub mod redis_client;
pub mod rows;
pub mod tls;
pub mod value;

pub use postgres::{PyDatabase, PyTransaction};
pub use redis_client::PyRedis;
pub use rows::RawJson;
