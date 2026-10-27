//! Lumen's Jev evaluation harness (docs/ai/harness.md).
//!
//! Live model outputs are not deterministic, so reproducibility comes from
//! **cassettes**: recorded raw answers keyed by the exact request they answered.
//! [`ReplayJudge`] answers from a cassette through the same validator production
//! uses, so a replay exercises the real request hashing, validation and policy.
//! Mock mode uses `lumen_testkit::MockJudge`. [`run`] grades cases
//! (`jev.case/1`) against a judge and reports the critical false-positive gate;
//! [`load`] verifies a dataset version against its manifest and
//! [`run_dataset`] adds the injection-canary gate. The `lumen-jev-eval run`
//! command applies the gates to a dataset and a cassette (roadmap 12.1);
//! [`record`] writes a cassette from a live judge (`lumen-jev-eval record`,
//! roadmap 12.1b).

#![forbid(unsafe_code)]

mod cassette;
mod dataset;
mod record;
mod run;

pub use cassette::{CASSETTE_SCHEMA, Cassette, CassetteError, Recorded, RecordedStop, ReplayJudge};
pub use dataset::{Dataset, DatasetError, DatasetReport, load, run_dataset};
pub use record::{LiveError, LiveJudge, Pacing, RecordOutcome, Recording, record};
pub use run::{
    CASE_SCHEMA, CRITICAL_FPR_TARGET, Case, Expected, Grade, Outcome, Report, block_on,
    clopper_pearson_upper, run,
};
