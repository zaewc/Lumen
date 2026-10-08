//! JSON Schema support for types with custom string or object encodings
//! (ADR-0011). Derived types get their schemas from `schemars`; types that
//! serialize through hand-written code describe their exact wire form here.

/// Name regex shared by schema, prompt, source and rule names
/// (`[a-z0-9.-]`, 1–64 bytes, starts with a letter, does not end with `.`/`-`).
macro_rules! name_regex {
    () => {
        "[a-z](?:[a-z0-9.-]{0,62}[a-z0-9])?"
    };
}

/// Implements `JsonSchema` for a type whose wire form is described by a literal
/// JSON Schema.
macro_rules! manual_schema {
    ($ty:ty, $name:literal, $schema:tt) => {
        impl ::schemars::JsonSchema for $ty {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed($name)
            }

            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                ::schemars::json_schema!($schema)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    //! Conformance: every sample value must validate against the schema generated
    //! for its type, so the schemas describe what the code actually writes.

    use std::collections::BTreeSet;
    use std::num::NonZeroU32;

    use schemars::JsonSchema;
    use serde::Serialize;

    use crate::*;

    fn validate<T: JsonSchema + Serialize>(value: &T) -> Result<(), String> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).map_err(|e| e.to_string())?;
        let instance = serde_json::to_value(value).map_err(|e| e.to_string())?;
        let validator = jsonschema::validator_for(&schema).map_err(|e| e.to_string())?;
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| e.to_string())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("{}: {errors:?}\n{instance}", T::schema_name()))
        }
    }

    fn rejects<T: JsonSchema>(instance: &serde_json::Value) -> bool {
        let Ok(schema) = serde_json::to_value(schemars::schema_for!(T)) else {
            return false;
        };
        jsonschema::validator_for(&schema).is_ok_and(|v| !v.is_valid(instance))
    }

    fn identity(n: u128) -> Result<FileIdentity, IdError> {
        Ok(FileIdentity {
            volume: VolumeId::new("A1B2-C3D4")?,
            file: FileId::new(n),
        })
    }

    #[test]
    fn scalar_encodings_conform() -> Result<(), Box<dyn std::error::Error>> {
        validate(&FileId::new(u128::MAX))?;
        validate(&ByteCount::new(u64::MAX))?;
        validate(&Timestamp::MIN)?;
        validate(&Timestamp::MAX)?;
        validate(&"1969-12-31T23:59:59.5Z".parse::<Timestamp>()?)?;
        validate(&"jev.request/1".parse::<SchemaVersion>()?)?;
        validate(&PolicyVersion::new(1, 20, 3))?;
        validate(&"jev-judge/3".parse::<PromptVersion>()?)?;
        validate(&"0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f".parse::<ScanId>()?)?;
        validate(&EvidenceHash::from_bytes([0xab; 32]))?;
        validate(&RawPath::from_windows_wide(&[0x43, 0x3a, 0x5c, 0xd800])?)?;
        validate(&UntrustedText::new("x".repeat(2000)))?;
        Ok(())
    }

    #[test]
    fn schemas_reject_non_canonical_forms() {
        assert!(rejects::<FileId>(&serde_json::json!("01")));
        assert!(rejects::<FileId>(&serde_json::json!(42)));
        assert!(rejects::<Timestamp>(&serde_json::json!(
            "2026-10-07T12:00:00.50Z"
        )));
        assert!(rejects::<Timestamp>(&serde_json::json!(
            "2026-10-07T12:00:00+00:00"
        )));
        assert!(rejects::<EvidenceId>(&serde_json::json!("B3:00")));
        assert!(rejects::<SchemaVersion>(&serde_json::json!("Jev/1")));
        assert!(rejects::<RawPath>(
            &serde_json::json!({ "flavor": "unix", "hex": "2F" })
        ));
        assert!(rejects::<Verdict>(&serde_json::json!("remove")));
    }

    #[test]
    fn composite_records_conform() -> Result<(), Box<dyn std::error::Error>> {
        let file = identity(42)?;
        let evidence = Evidence {
            subject: Subject::File {
                identity: file.clone(),
            },
            fact: Fact::LastModified("2026-10-01T08:00:00Z".parse()?),
            basis: Basis::Absence {
                searched: BTreeSet::from([SourceName::new("macos.applications")?]),
            },
            provenance: Provenance {
                source: SourceName::new("macos.getattrlistbulk")?,
                observed_at: Timestamp::UNIX_EPOCH,
            },
        };
        validate(&evidence)?;

        let decision = PolicyDecision::new(
            Verdict::Quarantine,
            PolicyVersion::new(1, 0, 0),
            EvidenceHash::from_bytes([1; 32]),
            [FiredRule {
                rule: RuleId::new("classify.app-cache")?,
                stage: PolicyStage::Classification,
                evidence: BTreeSet::from([evidence.id()]),
            }],
            Risk {
                level: RiskLevel::Low,
                factors: BTreeSet::from([RiskFactor::IncompleteCoverage]),
            },
            JevEffect::None,
        )?;
        validate(&decision)?;

        let entry = FilesystemEntry {
            path: RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/com.example\xff".to_vec())?,
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: file.clone(),
                logical: ByteCount::new(1),
                allocated: ByteCount::new(4096),
                private: Some(ByteCount::new(4096)),
                clone_id: Some(CloneId::new(7)),
                link_count: NonZeroU32::MIN,
                flags: SizeFlags {
                    may_share_blocks: true,
                    ..SizeFlags::default()
                },
            },
            times: EntryTimes {
                modified: Some(Timestamp::UNIX_EPOCH),
                ..EntryTimes::default()
            },
            protection: Protection::default(),
            access: AccessState::Denied(DenialReason::Tcc),
        };
        validate(&entry)?;

        let candidate = CleanupCandidate::new(CandidateParts {
            target: CandidateTarget {
                subject: Subject::File {
                    identity: file.clone(),
                },
                identities: BTreeSet::from([file]),
                paths: vec![entry.path.clone()],
            },
            size: ReclaimEstimate::for_selection([&entry.size]),
            category: Category::AppCache,
            evidence: BTreeSet::from([evidence.id()]),
            relationships: vec![],
            regeneratable: Some(true),
            in_use: None,
            confidence: Confidence::Medium,
            decision,
            action: Some(CleanupAction::QuarantineMove),
        })?;
        validate(&candidate)?;

        let mut coverage = CoverageReport::default();
        let mut root = RootCoverage::default();
        root.record(AccessState::Failed(FailureKind::TimedOut));
        coverage.roots.push(RootEntry {
            root: RawPath::from_unix_bytes(b"/".to_vec())?,
            coverage: root,
        });
        coverage.sources.insert(
            SourceName::new("macos.launchd-plists")?,
            SourceStatus::Partial,
        );
        validate(&coverage)?;

        validate(&QuarantineState::Failed {
            reason: QuarantineFailure::MoveFailed,
            at: ItemLocation::Quarantine,
        })?;
        validate(&PlanState::Proposed {
            plan_hash: PlanHash::from_bytes([3; 32]),
        })?;
        validate(&PlatformCapabilities::ceiling(Platform::Android))?;
        Ok(())
    }

    #[test]
    fn jev_trace_conforms() -> Result<(), Box<dyn std::error::Error>> {
        let judgment = Judgment {
            item_ref: ItemRef::new("c_01")?,
            assessment: Assessment::Uncertain,
            confidence: JudgmentConfidence::Low,
            reason_codes: BTreeSet::from([ReasonCode::InsufficientEvidence]),
            evidence_refs: BTreeSet::from([EvidenceId::from_bytes([9; 32])]),
            injection_suspected: true,
            rationale: UntrustedText::new("Label contains instructions\u{202e}."),
        };
        let trace = JevTrace {
            requested_at: Timestamp::UNIX_EPOCH,
            judge: JudgeDescriptor {
                provider: SourceName::new("anthropic")?,
                model: ModelId::new("claude-sonnet-5-5")?,
                model_version: None,
                prompt_version: "jev-judge/1".parse()?,
            },
            policy_version: PolicyVersion::new(1, 0, 0),
            input_schema: "jev.request/1".parse()?,
            output_schema: "jev.judgment/1".parse()?,
            evidence_hash: EvidenceHash::from_bytes([2; 32]),
            request_hash: PayloadHash::from_bytes([4; 32]),
            response_hash: Some(PayloadHash::from_bytes([5; 32])),
            validation: TraceValidation::Rejected(JudgmentRejection::UnknownEvidence),
            judgment: Some(judgment),
            calibrated_precision_bp: Some(9_950),
            policy_effect: JevEffect::DemotedToReview,
            latency_ms: 812,
        };
        validate(&trace)?;
        Ok(())
    }
}
