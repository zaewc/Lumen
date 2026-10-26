//! Evidence guards (ADR-0043): a `likely_removable` answer that the request's
//! own evidence does not support is discarded.
//!
//! A guard runs after an answer has been validated against its request, for
//! every judge, live and in replay. When one fires, the assessment becomes
//! `uncertain` with `low` confidence; the reasons, evidence references and the
//! injection flag stay as the judge gave them. Guards never touch
//! `likely_needed` or `uncertain`, so they can only make a decision more
//! cautious (ADR-0020). The guards, in the order they are checked:
//!
//! 1. [`Guard::NotRegeneratable`]: the request says the item cannot be
//!    regenerated;
//! 2. [`Guard::ContradictedReason`]: a cited reason contradicts a fact the
//!    request states (the owner is installed, a process uses it);
//! 3. [`Guard::NoReasonForRemoval`]: no cited reason argues for removal, or one
//!    argues against it;
//! 4. [`Guard::UndeclaredVendorCache`]: a vendor declaration is cited that the
//!    request does not contain, so only untrusted text can have suggested it;
//! 5. [`Guard::InjectionSignal`]: the judge saw signs of text trying to steer
//!    the decision.
//!
//! The evidence for each, measured on the recorded Jeb answers, is in
//! `docs/research/17-jeb-error-analysis.md`.

use lumen_domain::{
    Assessment, EvidenceKind, JevRequest, Judgment, JudgmentConfidence, ReasonCode,
};

/// Why a removable answer was discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// The request says `regeneratable: false`.
    NotRegeneratable,
    /// A cited reason contradicts a fact the request states.
    ContradictedReason,
    /// No cited reason argues for removal, or one argues against it.
    NoReasonForRemoval,
    /// `vendor_declared_cache` is cited, but the request holds no vendor
    /// declaration.
    UndeclaredVendorCache,
    /// The judge suspects, even weakly, that text tries to steer the decision.
    InjectionSignal,
}

/// Reasons that argue for removal.
const FOR_REMOVAL: [ReasonCode; 6] = [
    ReasonCode::OwnerAppAbsent,
    ReasonCode::RegeneratableCache,
    ReasonCode::NoActiveReference,
    ReasonCode::VendorDeclaredCache,
    ReasonCode::KnowledgeBaseMatch,
    ReasonCode::StaleByTime,
];

/// Reasons that argue against removal, or against concluding anything. The
/// rest (`unsigned_executable`) argue neither way.
const AGAINST_REMOVAL: [ReasonCode; 10] = [
    ReasonCode::OwnerAppPresent,
    ReasonCode::NotRegeneratable,
    ReasonCode::ActiveReference,
    ReasonCode::RecentlyUsed,
    ReasonCode::ContainsUserData,
    ReasonCode::CredentialOrKeyMaterial,
    ReasonCode::SystemComponent,
    ReasonCode::CloudBacked,
    ReasonCode::InsufficientEvidence,
    ReasonCode::SuspiciousMetadata,
];

/// The guard that discards `judgment`'s removable assessment of `request`, if
/// any. `injection_signal` is the judge's own sign of injection, which may be
/// weaker than [`Judgment::injection_suspected`] (a Decisions judge's
/// probability from 0.1, see `decisions`).
pub fn check(request: &JevRequest, judgment: &Judgment, injection_signal: bool) -> Option<Guard> {
    if judgment.assessment != Assessment::LikelyRemovable {
        return None;
    }
    let states = |kind: EvidenceKind, value: &str| {
        request
            .evidence
            .iter()
            .any(|e| e.kind == kind && e.value.raw() == value)
    };
    let holds = |kind: EvidenceKind| request.evidence.iter().any(|e| e.kind == kind);
    let cites = |reason: ReasonCode| judgment.reason_codes.contains(&reason);
    if states(EvidenceKind::Regeneratable, "false") {
        Some(Guard::NotRegeneratable)
    } else if (cites(ReasonCode::OwnerAppAbsent)
        && states(EvidenceKind::ApplicationInstalled, "true")
        && !states(EvidenceKind::ApplicationInstalled, "false"))
        || (cites(ReasonCode::NoActiveReference)
            && states(EvidenceKind::ActiveProcessReference, "true"))
    {
        Some(Guard::ContradictedReason)
    } else if !FOR_REMOVAL.into_iter().any(cites) || AGAINST_REMOVAL.into_iter().any(cites) {
        Some(Guard::NoReasonForRemoval)
    } else if cites(ReasonCode::VendorDeclaredCache) && !holds(EvidenceKind::VendorDeclaredCache) {
        Some(Guard::UndeclaredVendorCache)
    } else if injection_signal || judgment.injection_suspected {
        Some(Guard::InjectionSignal)
    } else {
        None
    }
}

/// `judgment` with its removable assessment discarded if a guard fires
/// ([`check`]): `uncertain` at `low` confidence, everything else unchanged.
pub fn apply(request: &JevRequest, judgment: Judgment, injection_signal: bool) -> Judgment {
    if check(request, &judgment, injection_signal).is_none() {
        return judgment;
    }
    Judgment {
        assessment: Assessment::Uncertain,
        confidence: JudgmentConfidence::Low,
        ..judgment
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use lumen_domain::{
        ByteCount, CandidateSummary, Category, CoverageSummary, EvidenceId, ItemRef,
        RequestEvidence, UntrustedText,
    };

    use super::*;
    use EvidenceKind as K;
    use ReasonCode as R;

    fn request(facts: &[(EvidenceKind, &str)]) -> JevRequest {
        JevRequest {
            schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
            item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
            candidate: CandidateSummary {
                category: Category::AppCache,
                path_display: UntrustedText::new("~/Library/Caches/x"),
                size_bytes: ByteCount::ZERO,
                reclaimable_bytes: ByteCount::ZERO,
            },
            evidence: facts
                .iter()
                .zip(1u8..)
                .map(|((kind, value), i)| RequestEvidence {
                    id: EvidenceId::from_bytes([i; 32]),
                    kind: *kind,
                    value: UntrustedText::new(*value),
                })
                .collect(),
            coverage: CoverageSummary {
                complete: true,
                notes: BTreeSet::new(),
            },
            untrusted_strings: BTreeMap::new(),
        }
    }

    fn judgment(assessment: Assessment, reasons: &[ReasonCode]) -> Judgment {
        Judgment {
            item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
            assessment,
            confidence: JudgmentConfidence::High,
            reason_codes: reasons.iter().copied().collect(),
            evidence_refs: BTreeSet::from([EvidenceId::from_bytes([1; 32])]),
            injection_suspected: false,
            rationale: UntrustedText::new(""),
        }
    }

    fn removable(reasons: &[ReasonCode]) -> Judgment {
        judgment(Assessment::LikelyRemovable, reasons)
    }

    #[test]
    fn a_supported_removable_answer_stands() {
        let req = request(&[
            (K::OwningApplication, "com.example"),
            (K::ApplicationInstalled, "false"),
            (K::AppDataLocation, "caches"),
            (K::Regeneratable, "true"),
            (
                K::VendorDeclaredCache,
                "Disk Cleanup handler: Temporary Files",
            ),
        ]);
        for reasons in [
            &[R::RegeneratableCache][..],
            &[R::OwnerAppAbsent],
            &[R::VendorDeclaredCache],
            &[R::StaleByTime],
            &[R::OwnerAppAbsent, R::UnsignedExecutable],
        ] {
            let answer = removable(reasons);
            assert_eq!(check(&req, &answer, false), None, "{reasons:?}");
            assert_eq!(apply(&req, answer.clone(), false), answer);
        }
    }

    #[test]
    fn regeneratable_false_discards_removal() {
        let req = request(&[(K::Regeneratable, "false")]);
        assert_eq!(
            check(&req, &removable(&[R::KnowledgeBaseMatch]), false),
            Some(Guard::NotRegeneratable)
        );
        let kept = apply(&req, removable(&[R::RegeneratableCache]), false);
        assert_eq!(kept.assessment, Assessment::Uncertain);
        assert_eq!(kept.confidence, JudgmentConfidence::Low);
        assert_eq!(kept.reason_codes, BTreeSet::from([R::RegeneratableCache]));
    }

    #[test]
    fn a_reason_the_request_contradicts_discards_removal() {
        let installed = request(&[(K::ApplicationInstalled, "true")]);
        assert_eq!(
            check(&installed, &removable(&[R::OwnerAppAbsent]), false),
            Some(Guard::ContradictedReason)
        );
        let in_use = request(&[(K::ActiveProcessReference, "true")]);
        assert_eq!(
            check(&in_use, &removable(&[R::NoActiveReference]), false),
            Some(Guard::ContradictedReason)
        );
        // One of two owners is gone: "owner absent" is not contradicted.
        let mixed = request(&[
            (K::ApplicationInstalled, "true"),
            (K::ApplicationInstalled, "false"),
        ]);
        assert_eq!(check(&mixed, &removable(&[R::OwnerAppAbsent]), false), None);
    }

    #[test]
    fn removal_needs_a_reason_for_removal() {
        let req = request(&[(K::OwningApplication, "x")]);
        for reasons in [
            &[R::UnsignedExecutable][..],
            &[R::RecentlyUsed],
            &[R::InsufficientEvidence],
            &[R::StaleByTime, R::ContainsUserData],
            &[R::OwnerAppAbsent, R::SuspiciousMetadata],
        ] {
            assert_eq!(
                check(&req, &removable(reasons), false),
                Some(Guard::NoReasonForRemoval),
                "{reasons:?}"
            );
        }
    }

    #[test]
    fn an_undeclared_vendor_cache_discards_removal() {
        let req = request(&[(K::OwningApplication, "x")]);
        assert_eq!(
            check(&req, &removable(&[R::VendorDeclaredCache]), false),
            Some(Guard::UndeclaredVendorCache)
        );
    }

    #[test]
    fn an_injection_signal_discards_removal() {
        let req = request(&[(K::OwningApplication, "x")]);
        let answer = removable(&[R::StaleByTime]);
        assert_eq!(check(&req, &answer, true), Some(Guard::InjectionSignal));
        let suspected = Judgment {
            injection_suspected: true,
            ..answer
        };
        assert_eq!(check(&req, &suspected, false), Some(Guard::InjectionSignal));
        let kept = apply(&req, suspected, false);
        assert_eq!(kept.assessment, Assessment::Uncertain);
        assert!(kept.injection_suspected, "the flag is kept");
    }

    #[test]
    fn guards_never_touch_other_assessments() {
        let req = request(&[(K::Regeneratable, "false")]);
        for assessment in [Assessment::LikelyNeeded, Assessment::Uncertain] {
            for reasons in [&[R::VendorDeclaredCache][..], &[R::UnsignedExecutable]] {
                let answer = judgment(assessment, reasons);
                assert_eq!(check(&req, &answer, true), None);
                assert_eq!(apply(&req, answer.clone(), true), answer);
            }
        }
    }
}
