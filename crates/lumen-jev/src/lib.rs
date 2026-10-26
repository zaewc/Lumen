//! Jev, Lumen's optional AI judge (ADR-0020, `docs/ai/`).
//!
//! Jev contributes evidence about ambiguous candidates and never decides: the
//! deterministic policy consumes a validated judgment and can only become more
//! cautious. This crate builds the metadata-only request a model sees
//! ([`request`]), validates text answers ([`response`]), and asks `TypeSafe`'s Jev
//! model through `OpenRouter`'s Decisions API ([`decisions`], ADR-0031). It
//! also asks what kind of item a review item is, as a note for people
//! (ADR-0041). The HTTPS transport is [`openrouter`].

#![forbid(unsafe_code)]

pub mod decisions;
pub mod openrouter;
pub mod request;
pub mod response;
