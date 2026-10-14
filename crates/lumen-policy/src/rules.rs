//! The policy's rules, in evaluation order within each stage. Protections,
//! eligibility and classification rules land in their own changes
//! (roadmap 7.2–7.5); until then every item is `Review`.

use crate::Rule;

/// Every rule the policy evaluates.
pub(crate) const RULES: &[Rule] = &[];
