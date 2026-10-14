//! Lumen's Jev evaluation harness (docs/ai/harness.md).
//!
//! Live model outputs are not deterministic, so reproducibility comes from
//! **cassettes**: recorded raw answers keyed by the exact request they answered.
//! [`ReplayJudge`] answers from a cassette through the same validator production
//! uses, so a replay exercises the real request hashing, validation and policy.
//! Mock mode uses `lumen_testkit::MockJudge`. [`run`] grades cases
//! (`jev.case/1`) against a judge and reports the critical false-positive gate;
//! datasets and the CLI follow.

#![forbid(unsafe_code)]

mod cassette;
mod run;

pub use cassette::{CASSETTE_SCHEMA, Cassette, CassetteError, Recorded, RecordedStop, ReplayJudge};
pub use run::{
    CASE_SCHEMA, CRITICAL_FPR_TARGET, Case, Expected, Grade, Outcome, Report, block_on,
    clopper_pearson_upper, run,
};
