//! Which judge service a Jev key belongs to (ADR-0031, ADR-0036).

/// The service a key is for. Jeb is the default: the owner's school AI Hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Service {
    /// Jeb on the school AI Hub (ADR-0036).
    Jeb,
    /// `TypeSafe`'s Jev through `OpenRouter` (ADR-0031).
    OpenRouter,
}
