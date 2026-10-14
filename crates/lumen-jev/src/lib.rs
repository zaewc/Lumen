//! Jev, Lumen's optional AI judge (ADR-0020, `docs/ai/`).
//!
//! Jev contributes evidence about ambiguous candidates and never decides: the
//! deterministic policy consumes a validated judgment and can only become more
//! cautious. This crate builds the metadata-only request a model sees
//! ([`request`]), validates text answers ([`response`]), and asks `TypeSafe`'s Jev
//! model through `OpenRouter`'s Decisions API ([`decisions`], ADR-0031). The
//! network transport follows.

#![forbid(unsafe_code)]

pub mod decisions;
pub mod request;
pub mod response;
