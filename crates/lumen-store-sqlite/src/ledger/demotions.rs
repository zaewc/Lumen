//! Items AI review moved to review (audit finding AI-4): each judgment that
//! demoted a proposed quarantine, with the item's identity, recorded by the
//! application with the traces. Making a plan reads them back, so an item the
//! judge moved to review stays out of the plan even if the plan's own request
//! fails or is answered differently. A judgment can only add caution
//! (ADR-0020), so carrying one forward never makes a decision less cautious.

use lumen_domain::{FileIdentity, Judgment, Timestamp};
use rusqlite::params;

use super::LedgerStore;
use crate::StoreError;

impl LedgerStore {
    /// Records that `judgment`, requested at `requested_at`, demoted the item
    /// with `identity` to review.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn record_demotion(
        &mut self,
        requested_at: Timestamp,
        identity: &FileIdentity,
        judgment: &Judgment,
    ) -> Result<(), StoreError> {
        let identity = serde_json::to_vec(identity)
            .map_err(|_| StoreError::corrupt("jev_demotions.identity"))?;
        let judgment = serde_json::to_vec(judgment)
            .map_err(|_| StoreError::corrupt("jev_demotions.judgment"))?;
        self.conn.execute(
            "INSERT INTO jev_demotions (requested_secs, requested_nanos, identity, judgment) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                requested_at.unix_seconds(),
                requested_at.subsec_nanos(),
                identity,
                judgment
            ],
        )?;
        Ok(())
    }

    /// Every demotion requested at or after `since`, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for a row that does not validate, or
    /// [`StoreError`] if SQLite fails.
    pub fn demotions_since(
        &self,
        since: Timestamp,
    ) -> Result<Vec<(FileIdentity, Judgment)>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT identity, judgment FROM jev_demotions \
             WHERE requested_secs > ?1 OR (requested_secs = ?1 AND requested_nanos >= ?2) \
             ORDER BY seq",
        )?;
        let rows = query.query_map(params![since.unix_seconds(), since.subsec_nanos()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|row| {
            let (identity, judgment) = row?;
            Ok((
                serde_json::from_slice(&identity)
                    .map_err(|_| StoreError::corrupt("jev_demotions.identity"))?,
                serde_json::from_slice(&judgment)
                    .map_err(|_| StoreError::corrupt("jev_demotions.judgment"))?,
            ))
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{
        Assessment, FileId, ItemRef, JudgmentConfidence, ReasonCode, UntrustedText, VolumeId,
    };
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_ledger;

    fn identity(n: u128) -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(n),
        }
    }

    fn judgment() -> Judgment {
        Judgment {
            item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
            assessment: Assessment::LikelyNeeded,
            confidence: JudgmentConfidence::High,
            reason_codes: BTreeSet::from([ReasonCode::OwnerAppPresent]),
            evidence_refs: BTreeSet::new(),
            injection_suspected: false,
            rationale: UntrustedText::new("needed"),
        }
    }

    #[test]
    fn demotions_are_read_back_from_a_time_on() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let mut ledger = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        assert_eq!(ledger.demotions_since(Timestamp::UNIX_EPOCH)?, Vec::new());
        ledger.record_demotion(Timestamp::from_unix(10, 0)?, &identity(1), &judgment())?;
        ledger.record_demotion(Timestamp::from_unix(20, 5)?, &identity(2), &judgment())?;
        assert_eq!(
            ledger.demotions_since(Timestamp::UNIX_EPOCH)?,
            [(identity(1), judgment()), (identity(2), judgment())]
        );
        assert_eq!(
            ledger.demotions_since(Timestamp::from_unix(20, 5)?)?,
            [(identity(2), judgment())],
            "inclusive"
        );
        assert_eq!(
            ledger.demotions_since(Timestamp::from_unix(20, 6)?)?,
            Vec::new()
        );
        let tampered = ledger.conn.execute("DELETE FROM jev_demotions", []);
        assert!(tampered.is_err(), "append-only");
        Ok(())
    }

    #[test]
    fn a_tampered_row_is_reported() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let ledger = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        ledger.conn.execute(
            "INSERT INTO jev_demotions (requested_secs, requested_nanos, identity, judgment) \
             VALUES (1, 0, X'7B7D', X'7B7D')",
            [],
        )?;
        assert!(matches!(
            ledger.demotions_since(Timestamp::UNIX_EPOCH),
            Err(StoreError::Corrupt { .. })
        ));
        Ok(())
    }
}
