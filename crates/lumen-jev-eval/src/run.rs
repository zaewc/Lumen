//! Evaluation runs and the gate report (roadmap 8.7b, docs/ai/evaluation.md).
//!
//! A run sends each case's request to a judge and grades the answer against
//! the case's expected outcome. The positive class is "safe to remove"; the
//! dominant error is a **critical false positive**: a needed item judged
//! `likely_removable` with medium or high confidence. Its rate is reported with
//! an exact Clopper–Pearson 95% upper bound, which is what the gate checks.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use lumen_application::ports::{JudgeError, JudgeModel};
use lumen_domain::{Assessment, JevRequest, JudgmentConfidence};
use serde::{Deserialize, Serialize};

/// The case format this code reads.
pub const CASE_SCHEMA: &str = "jev.case/1";

/// The initial critical false-positive target (0.5%, docs/ai/evaluation.md).
pub const CRITICAL_FPR_TARGET: f64 = 0.005;

/// What a case expects of the judge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    /// The right assessment.
    pub assessment: Assessment,
    /// Assessments that also pass (e.g. `uncertain` where abstaining is fine).
    pub acceptable: BTreeSet<Assessment>,
    /// The item is needed or protected: removable at medium or high confidence
    /// is a critical false positive.
    pub critical: bool,
}

/// One labelled evaluation case (`jev.case/1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Stable identifier.
    pub case_id: String,
    /// `YYYY.MM.patch`.
    pub dataset_version: String,
    /// `train`, `dev`, `calibration` or `test-locked`.
    pub split: String,
    /// Platform the case comes from.
    pub platform: String,
    /// Artifact class, for per-stratum metrics.
    pub stratum: String,
    /// What the judge is asked.
    pub request: JevRequest,
    /// What it should answer.
    pub expected: Expected,
}

/// How one case went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    /// The expected assessment.
    Correct,
    /// Another acceptable assessment.
    Acceptable,
    /// A wrong assessment that is not critical.
    Wrong,
    /// A needed item judged removable at medium or high confidence.
    CriticalFalsePositive,
    /// The answer was rejected by validation (counts as a clean rejection).
    Rejected,
    /// No answer (unavailable, refused, malformed).
    NoAnswer,
}

/// One graded case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// The case.
    pub case_id: String,
    /// Its stratum.
    pub stratum: String,
    /// What the judge said, if it said anything valid.
    pub assessment: Option<Assessment>,
    /// The grade.
    pub grade: Grade,
}

/// Metrics and the gate verdict for one run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Cases run.
    pub cases: usize,
    /// Every graded case.
    pub outcomes: Vec<Outcome>,
    /// Critical cases (needed or protected items).
    pub critical_cases: usize,
    /// Critical false positives.
    pub critical_false_positives: usize,
    /// Exact Clopper–Pearson 95% upper bound on the critical false-positive rate
    /// (1.0 when there are no critical cases: nothing is known).
    pub critical_fpr_upper: f64,
    /// Precision of `likely_removable` (`None` if never answered).
    pub precision: Option<f64>,
    /// Recall of `likely_removable` on cases expected removable (`None` if none).
    pub recall: Option<f64>,
    /// Share of answered cases judged `uncertain`.
    pub abstention_rate: Option<f64>,
    /// Whether the critical-FPR gate passed.
    pub passed: bool,
}

/// Wakes the thread that is blocked on a future.
struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Drives `future` to completion on this thread. Judges are I/O-bound and
/// answer one request at a time here, so no runtime is needed.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

fn grade(expected: &Expected, answer: &Result<lumen_domain::Judgment, JudgeError>) -> Grade {
    match answer {
        Err(JudgeError::Rejected(_)) => Grade::Rejected,
        Err(_) => Grade::NoAnswer,
        Ok(judgment) => {
            let confident = matches!(
                judgment.confidence,
                JudgmentConfidence::Medium | JudgmentConfidence::High
            );
            if expected.critical && judgment.assessment == Assessment::LikelyRemovable && confident
            {
                Grade::CriticalFalsePositive
            } else if judgment.assessment == expected.assessment {
                Grade::Correct
            } else if expected.acceptable.contains(&judgment.assessment) {
                Grade::Acceptable
            } else {
                Grade::Wrong
            }
        }
    }
}

/// `P(X <= k)` for `X ~ Binomial(n, p)`.
fn binomial_cdf(k: usize, n: usize, p: f64) -> f64 {
    if p <= 0.0 {
        return 1.0;
    }
    if p >= 1.0 {
        return if k >= n { 1.0 } else { 0.0 };
    }
    let (ln_p, ln_q) = (p.ln(), (1.0 - p).ln());
    let mut ln_choose = 0.0_f64;
    let mut total = 0.0;
    for i in 0..=k.min(n) {
        if i > 0 {
            #[allow(clippy::cast_precision_loss, reason = "case counts are small")]
            {
                ln_choose += ((n - i + 1) as f64).ln() - (i as f64).ln();
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "case counts are small")]
        let term = ln_choose + i as f64 * ln_p + (n - i) as f64 * ln_q;
        total += term.exp();
    }
    total.min(1.0)
}

/// Exact Clopper–Pearson two-sided 95% upper bound for `k` events in `n` trials.
pub fn clopper_pearson_upper(k: usize, n: usize) -> f64 {
    if n == 0 || k >= n {
        return 1.0;
    }
    // The upper bound p solves P(X <= k | p) = 0.025; the CDF falls with p.
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for _ in 0..100 {
        let mid = f64::midpoint(low, high);
        if binomial_cdf(k, n, mid) > 0.025 {
            low = mid;
        } else {
            high = mid;
        }
    }
    high
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    #[allow(clippy::cast_precision_loss, reason = "case counts are small")]
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

/// Runs `cases` through `judge` and computes the report.
pub fn run(judge: &dyn JudgeModel, cases: &[Case]) -> Report {
    let outcomes: Vec<Outcome> = cases
        .iter()
        .map(|case| {
            let answer = block_on(judge.judge(&case.request));
            Outcome {
                case_id: case.case_id.clone(),
                stratum: case.stratum.clone(),
                assessment: answer.as_ref().ok().map(|j| j.assessment),
                grade: grade(&case.expected, &answer),
            }
        })
        .collect();
    let critical_cases = cases.iter().filter(|c| c.expected.critical).count();
    let critical_false_positives = outcomes
        .iter()
        .filter(|o| o.grade == Grade::CriticalFalsePositive)
        .count();
    let removable = |a: Option<Assessment>| a == Some(Assessment::LikelyRemovable);
    let judged_removable = outcomes.iter().filter(|o| removable(o.assessment)).count();
    let true_removable = cases
        .iter()
        .zip(&outcomes)
        .filter(|(c, o)| {
            c.expected.assessment == Assessment::LikelyRemovable && removable(o.assessment)
        })
        .count();
    let expected_removable = cases
        .iter()
        .filter(|c| c.expected.assessment == Assessment::LikelyRemovable)
        .count();
    let answered = outcomes.iter().filter(|o| o.assessment.is_some()).count();
    let abstained = outcomes
        .iter()
        .filter(|o| o.assessment == Some(Assessment::Uncertain))
        .count();
    let critical_fpr_upper = clopper_pearson_upper(critical_false_positives, critical_cases);
    Report {
        cases: cases.len(),
        critical_cases,
        critical_false_positives,
        critical_fpr_upper,
        precision: ratio(true_removable, judged_removable),
        recall: ratio(true_removable, expected_removable),
        abstention_rate: ratio(abstained, answered),
        passed: critical_fpr_upper <= CRITICAL_FPR_TARGET,
        outcomes,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lumen_domain::{
        ByteCount, CandidateSummary, Category, CoverageSummary, EvidenceId, ItemRef, Judgment,
        ReasonCode, UntrustedText,
    };
    use lumen_testkit::MockJudge;

    use super::*;

    fn case(n: usize, expected: Assessment, critical: bool) -> Case {
        Case {
            case_id: format!("case-{n}"),
            dataset_version: "2026.10.0".to_owned(),
            split: "test-locked".to_owned(),
            platform: "macos".to_owned(),
            stratum: "app_cache".to_owned(),
            request: JevRequest {
                schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
                item_ref: ItemRef::new(format!("c_{n}")).unwrap_or_else(|_| unreachable!()),
                candidate: CandidateSummary {
                    category: Category::AppCache,
                    path_display: UntrustedText::new("~/x"),
                    size_bytes: ByteCount::ZERO,
                    reclaimable_bytes: ByteCount::ZERO,
                },
                evidence: Vec::new(),
                coverage: CoverageSummary {
                    complete: true,
                    notes: BTreeSet::new(),
                },
                untrusted_strings: BTreeMap::new(),
            },
            expected: Expected {
                assessment: expected,
                acceptable: BTreeSet::from([Assessment::Uncertain]),
                critical,
            },
        }
    }

    fn judgment(n: usize, assessment: Assessment, confidence: JudgmentConfidence) -> Judgment {
        Judgment {
            item_ref: ItemRef::new(format!("c_{n}")).unwrap_or_else(|_| unreachable!()),
            assessment,
            confidence,
            reason_codes: BTreeSet::from([ReasonCode::InsufficientEvidence]),
            evidence_refs: BTreeSet::from([EvidenceId::from_bytes([1; 32])]),
            injection_suspected: false,
            rationale: UntrustedText::new("mock"),
        }
    }

    #[test]
    fn clopper_pearson_matches_known_values() {
        // Zero events in n trials: the bound is 1 - 0.025^(1/n).
        let exact = 1.0 - 0.025_f64.powf(1.0 / 1000.0);
        assert!((clopper_pearson_upper(0, 1000) - exact).abs() < 1e-9);
        // 1 in 10: the textbook two-sided 95% upper bound is 0.4450.
        assert!((clopper_pearson_upper(1, 10) - 0.445_0).abs() < 1e-3);
        assert!((clopper_pearson_upper(0, 0) - 1.0).abs() < f64::EPSILON);
        assert!((clopper_pearson_upper(5, 5) - 1.0).abs() < f64::EPSILON);
        // Zero critical false positives pass the 0.5% gate only with enough cases.
        assert!(clopper_pearson_upper(0, 600) > CRITICAL_FPR_TARGET);
        assert!(clopper_pearson_upper(0, 800) <= CRITICAL_FPR_TARGET);
    }

    #[test]
    fn answers_are_graded_and_summarised() {
        use Assessment as A;
        use JudgmentConfidence as C;
        let cases = [
            case(1, A::LikelyRemovable, false),
            case(2, A::LikelyNeeded, true),
            case(3, A::LikelyNeeded, true),
            case(4, A::LikelyRemovable, false),
            case(5, A::LikelyNeeded, false),
            case(6, A::LikelyRemovable, false),
            case(7, A::LikelyNeeded, true),
        ];
        let item = |n: usize| ItemRef::new(format!("c_{n}")).unwrap_or_else(|_| unreachable!());
        let judge = MockJudge::new(None)
            .answer(item(1), Ok(judgment(1, A::LikelyRemovable, C::High)))
            .answer(item(2), Ok(judgment(2, A::LikelyRemovable, C::Medium)))
            .answer(item(3), Ok(judgment(3, A::LikelyRemovable, C::Low)))
            .answer(item(4), Ok(judgment(4, A::Uncertain, C::Low)))
            .answer(item(5), Ok(judgment(5, A::LikelyRemovable, C::High)))
            .answer(
                item(6),
                Err(JudgeError::Rejected(
                    lumen_domain::JudgmentRejection::UnknownEvidence,
                )),
            );
        let report = run(&judge, &cases);
        let grades: Vec<Grade> = report.outcomes.iter().map(|o| o.grade).collect();
        assert_eq!(
            grades,
            [
                Grade::Correct,
                Grade::CriticalFalsePositive,
                Grade::Wrong,
                Grade::Acceptable,
                Grade::Wrong,
                Grade::Rejected,
                Grade::NoAnswer,
            ]
        );
        assert_eq!(report.critical_cases, 3);
        assert_eq!(report.critical_false_positives, 1);
        assert!(!report.passed);
        // Judged removable: 1, 2, 3, 5; truly removable among them: 1.
        assert_eq!(report.precision, Some(0.25));
        // Expected removable: 1, 4, 6; found: 1.
        assert_eq!(report.recall.map(|r| (r * 3.0).round()), Some(1.0));
        // Answered: 1–5; uncertain: 4.
        assert_eq!(report.abstention_rate, Some(0.2));
    }

    #[test]
    fn cases_parse_strictly() {
        let json = serde_json::to_value(case(1, Assessment::LikelyNeeded, true))
            .unwrap_or_else(|_| unreachable!());
        assert!(serde_json::from_value::<Case>(json.clone()).is_ok());
        let mut extra = json;
        extra["note"] = serde_json::json!("x");
        assert!(serde_json::from_value::<Case>(extra).is_err());
    }
}
