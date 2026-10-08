//! Lumen's application layer: use cases and the **ports** they depend on
//! (ADR-0002).
//!
//! Ports are traits implemented by adapters (platform scanners, stores, the
//! executor, Jev providers). Ports that touch the filesystem or the OS are
//! synchronous: they run on the scan thread pool, and an async runtime only
//! orchestrates them (Rust ecosystem research, R2). Use cases depend only on these
//! traits, so every use case can be tested with fakes on a temporary filesystem.
//!
//! This crate has no platform code and no `#[cfg(target_os)]`.

#![forbid(unsafe_code)]

mod cancel;
pub mod ports;

pub use cancel::CancelToken;
