//! Jev, Lumen's optional AI judge (ADR-0020, `docs/ai/`).
//!
//! Jev contributes evidence about ambiguous candidates and never decides: the
//! deterministic policy consumes a validated judgment and can only become more
//! cautious. This crate builds the metadata-only request a model sees
//! ([`request`]); response validation and provider adapters follow.

#![forbid(unsafe_code)]

pub mod request;
