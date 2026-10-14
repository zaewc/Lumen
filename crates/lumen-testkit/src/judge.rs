//! A scripted [`JudgeModel`] for tests and the evaluation harness's mock mode.

use std::collections::BTreeMap;

use lumen_application::ports::{JudgeError, JudgeFuture, JudgeModel};
use lumen_domain::{ItemRef, JevRequest, JudgeDescriptor, Judgment};

/// Answers each request from a script keyed by `item_ref`; requests without an
/// entry are [`JudgeError::Unavailable`]. Never touches a network.
#[derive(Debug, Clone, Default)]
pub struct MockJudge {
    descriptor: Option<JudgeDescriptor>,
    script: BTreeMap<ItemRef, Result<Judgment, JudgeError>>,
}

impl MockJudge {
    /// A judge that reports `descriptor` and answers nothing yet.
    pub fn new(descriptor: Option<JudgeDescriptor>) -> Self {
        Self {
            descriptor,
            script: BTreeMap::new(),
        }
    }

    /// Answers `item` with `answer`.
    #[must_use]
    pub fn answer(mut self, item: ItemRef, answer: Result<Judgment, JudgeError>) -> Self {
        self.script.insert(item, answer);
        self
    }
}

impl JudgeModel for MockJudge {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        self.descriptor.clone()
    }

    fn judge<'a>(&'a self, request: &'a JevRequest) -> JudgeFuture<'a> {
        let answer = self
            .script
            .get(&request.item_ref)
            .cloned()
            .unwrap_or(Err(JudgeError::Unavailable("not scripted")));
        Box::pin(std::future::ready(answer))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::task::{Context, Poll, Waker};

    use lumen_domain::{
        Assessment, ByteCount, CandidateSummary, Category, CoverageSummary, JudgmentConfidence,
        ReasonCode, UntrustedText,
    };

    use super::*;

    fn request(item: &str) -> Result<JevRequest, Box<dyn std::error::Error>> {
        Ok(JevRequest {
            schema_version: JevRequest::current_schema()?,
            item_ref: ItemRef::new(item)?,
            candidate: CandidateSummary {
                category: Category::Other,
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
        })
    }

    fn poll(judge: &dyn JudgeModel, request: &JevRequest) -> Poll<Result<Judgment, JudgeError>> {
        judge
            .judge(request)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn answers_from_its_script() -> Result<(), Box<dyn std::error::Error>> {
        let judgment = Judgment {
            item_ref: ItemRef::new("c_1")?,
            assessment: Assessment::LikelyNeeded,
            confidence: JudgmentConfidence::Medium,
            reason_codes: BTreeSet::from([ReasonCode::OwnerAppPresent]),
            evidence_refs: BTreeSet::new(),
            injection_suspected: false,
            rationale: UntrustedText::new("scripted"),
        };
        let judge = MockJudge::new(None)
            .answer(ItemRef::new("c_1")?, Ok(judgment.clone()))
            .answer(ItemRef::new("c_2")?, Err(JudgeError::Malformed));
        assert_eq!(poll(&judge, &request("c_1")?), Poll::Ready(Ok(judgment)));
        assert_eq!(
            poll(&judge, &request("c_2")?),
            Poll::Ready(Err(JudgeError::Malformed))
        );
        assert!(matches!(
            poll(&judge, &request("c_3")?),
            Poll::Ready(Err(JudgeError::Unavailable(_)))
        ));
        Ok(())
    }
}
