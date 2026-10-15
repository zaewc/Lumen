//! Cleanup plans in `index.db` (roadmap 10.11, ADR-0034).
//!
//! A plan is saved once, proposed, with one decision per item. Its state then
//! changes only through [`IndexStore::apply_plan_event`], which applies the
//! domain's lifecycle (a confirmation must name the proposed plan's hash) in
//! the same transaction that records the new state. On every read the stored
//! body is hashed again and must match the stored hash.

use lumen_domain::{CleanupPlan, PlanEvent, PlanId, PlanState, PolicyDecision};
use rusqlite::{OptionalExtension, params};

use super::IndexStore;
use crate::StoreError;

/// A stored plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPlan {
    /// The plan as proposed.
    pub plan: CleanupPlan,
    /// Its lifecycle state.
    pub state: PlanState,
    /// The decision behind each item, in item order.
    pub decisions: Vec<PolicyDecision>,
}

fn json<T: serde::Serialize>(column: &'static str, value: &T) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec(value).map_err(|_| StoreError::Corrupt { column })
}

fn parse<T: serde::de::DeserializeOwned>(
    column: &'static str,
    bytes: &[u8],
) -> Result<T, StoreError> {
    serde_json::from_slice(bytes).map_err(|_| StoreError::Corrupt { column })
}

impl IndexStore {
    /// Saves `plan`, proposed, with `decisions` (one per item, in item order).
    ///
    /// # Errors
    ///
    /// [`StoreError::PlanMismatch`] if the decisions do not match the items,
    /// or [`StoreError`] if SQLite fails (including a plan ID saved before).
    pub fn save_plan(
        &mut self,
        plan: &CleanupPlan,
        decisions: &[PolicyDecision],
    ) -> Result<(), StoreError> {
        if decisions.len() != plan.items().len() {
            return Err(StoreError::PlanMismatch);
        }
        let hash = plan.hash();
        let state = PlanState::Proposed { plan_hash: hash };
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO plans (id, hash, body, state) VALUES (?1, ?2, ?3, ?4)",
            params![
                plan.id().to_string(),
                hash.to_string(),
                plan.canonical_bytes(),
                json("plans.state", &state)?,
            ],
        )?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO plan_decisions (plan_id, position, decision) VALUES (?1, ?2, ?3)",
            )?;
            for (position, decision) in decisions.iter().enumerate() {
                insert.execute(params![
                    plan.id().to_string(),
                    i64::try_from(position).map_err(|_| StoreError::OutOfRange {
                        column: "plan_decisions.position"
                    })?,
                    json("plan_decisions.decision", decision)?,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The plan with `id`, if stored.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupt`] if the stored body no longer matches its hash
    /// or any value fails validation, or [`StoreError`] if SQLite fails.
    pub fn plan(&self, id: PlanId) -> Result<Option<StoredPlan>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT hash, body, state FROM plans WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((hash, body, state)) = row else {
            return Ok(None);
        };
        let plan: CleanupPlan = parse("plans.body", &body)?;
        if plan.hash().to_string() != hash || plan.canonical_bytes() != body || plan.id() != id {
            return Err(StoreError::Corrupt {
                column: "plans.body",
            });
        }
        let state: PlanState = parse("plans.state", &state)?;
        let mut query = self.conn.prepare_cached(
            "SELECT decision FROM plan_decisions WHERE plan_id = ?1 ORDER BY position",
        )?;
        let decisions = query
            .query_map([id.to_string()], |row| row.get::<_, Vec<u8>>(0))?
            .map(|bytes| parse("plan_decisions.decision", &bytes?))
            .collect::<Result<Vec<PolicyDecision>, StoreError>>()?;
        if decisions.len() != plan.items().len() {
            return Err(StoreError::Corrupt {
                column: "plan_decisions",
            });
        }
        Ok(Some(StoredPlan {
            plan,
            state,
            decisions,
        }))
    }

    /// Applies `event` to the plan's lifecycle and records the new state.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownPlan`], [`StoreError::PlanTransition`] if the
    /// lifecycle forbids the event (including a confirmation for another
    /// plan), or [`StoreError`] if SQLite fails.
    pub fn apply_plan_event(
        &mut self,
        id: PlanId,
        event: PlanEvent,
    ) -> Result<PlanState, StoreError> {
        let tx = self.conn.transaction()?;
        let current: Option<Vec<u8>> = tx
            .query_row(
                "SELECT state FROM plans WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let current: PlanState = match current {
            Some(bytes) => parse("plans.state", &bytes)?,
            None => return Err(StoreError::UnknownPlan { plan: id }),
        };
        let next = current.apply(event)?;
        tx.execute(
            "UPDATE plans SET state = ?2 WHERE id = ?1",
            params![id.to_string(), json("plans.state", &next)?],
        )?;
        tx.commit()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::NonZeroU32;

    use lumen_domain::{
        ByteCount, Confirmation, ConfirmationSurface, EvidenceHash, FileId, FileIdentity,
        FiredRule, JevEffect, PlanHash, PlannedItem, PolicyStage, PolicyVersion, RawPath,
        ReclaimEstimate, Risk, RiskLevel, RuleId, SizeFacts, SizeFlags, Subject, Timestamp,
        Verdict, VolumeId,
    };
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_index;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn decision() -> Result<PolicyDecision, Box<dyn std::error::Error>> {
        Ok(PolicyDecision::new(
            Verdict::Quarantine,
            PolicyVersion::new(0, 10, 0),
            EvidenceHash::from_bytes([1; 32]),
            [FiredRule {
                rule: RuleId::new("classify.app-cache")?,
                stage: PolicyStage::Classification,
                evidence: BTreeSet::new(),
            }],
            Risk {
                level: RiskLevel::Low,
                factors: BTreeSet::new(),
            },
            JevEffect::None,
        )?)
    }

    fn plan(id: &str) -> Result<CleanupPlan, Box<dyn std::error::Error>> {
        let identity = FileIdentity {
            volume: VolumeId::new("v")?,
            file: FileId::new(1),
        };
        let size = SizeFacts {
            identity: identity.clone(),
            logical: ByteCount::new(4096),
            allocated: ByteCount::new(4096),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags::default(),
        };
        Ok(CleanupPlan::new(
            id.parse()?,
            Timestamp::UNIX_EPOCH,
            vec![PlannedItem {
                subject: Subject::File { identity },
                path: RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/x".to_vec())?,
                policy_version: PolicyVersion::new(0, 10, 0),
                evidence_hash: EvidenceHash::from_bytes([1; 32]),
                reclaim: ReclaimEstimate::for_selection([&size]),
            }],
        ))
    }

    const ID: &str = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3";

    #[test]
    fn a_saved_plan_reads_back_proposed_and_confirms_only_by_its_hash() -> TestResult {
        let fx = Fixture::build(&[])?;
        let mut store = IndexStore::new(open_index(&fx.path("index.db")?)?);
        let plan = plan(ID)?;
        store.save_plan(&plan, &[decision()?])?;
        let stored = store.plan(plan.id())?.ok_or("missing")?;
        assert_eq!(stored.plan, plan);
        assert_eq!(stored.decisions, [decision()?]);
        assert_eq!(
            stored.state,
            PlanState::Proposed {
                plan_hash: plan.hash()
            }
        );
        let confirm = |plan_hash| PlanEvent::Confirm {
            confirmation: Confirmation {
                plan_hash,
                confirmed_at: Timestamp::UNIX_EPOCH,
                surface: ConfirmationSurface::CommandLine,
            },
        };
        assert!(matches!(
            store.apply_plan_event(plan.id(), confirm(PlanHash::from_bytes([9; 32]))),
            Err(StoreError::PlanTransition(_))
        ));
        assert!(matches!(
            store.apply_plan_event(plan.id(), confirm(plan.hash()))?,
            PlanState::Confirmed { .. }
        ));
        assert!(matches!(
            store.apply_plan_event(plan.id(), PlanEvent::Start)?,
            PlanState::Executing { .. }
        ));
        Ok(())
    }

    #[test]
    fn mismatched_unknown_and_tampered_plans_are_errors() -> TestResult {
        let fx = Fixture::build(&[])?;
        let mut store = IndexStore::new(open_index(&fx.path("index.db")?)?);
        let plan = plan(ID)?;
        assert!(matches!(
            store.save_plan(&plan, &[]),
            Err(StoreError::PlanMismatch)
        ));
        assert_eq!(store.plan(plan.id())?, None);
        assert!(matches!(
            store.apply_plan_event(plan.id(), PlanEvent::Start),
            Err(StoreError::UnknownPlan { .. })
        ));
        store.save_plan(&plan, &[decision()?])?;
        store
            .conn
            .execute("UPDATE plans SET hash = 'b3:00' WHERE id = ?1", [ID])?;
        assert!(matches!(
            store.plan(plan.id()),
            Err(StoreError::Corrupt {
                column: "plans.body"
            })
        ));
        Ok(())
    }
}
