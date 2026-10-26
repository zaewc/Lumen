//! Audit records in `ledger.db` (ADR-0012): append-only, `synchronous=FULL`.
//!
//! It holds Jev traces (`jev.trace/1`) and the quarantine journal
//! ([`JournalEntry`]); policy decisions and manifests join it later. Reads re-validate every row: the body must
//! parse as a trace and agree with its indexed columns, so a tampered row is
//! reported, never returned.

mod journal;
mod port;
mod sizes;

pub use journal::JournalEntry;
pub use port::LedgerJournal;

use lumen_domain::{EvidenceHash, JevTrace, PayloadHash, Timestamp};
use rusqlite::{Connection, params};

use crate::StoreError;

/// Audit records in `ledger.db`.
#[derive(Debug)]
pub struct LedgerStore {
    conn: Connection,
}

fn corrupt(column: &'static str) -> StoreError {
    StoreError::Corrupt { column }
}

impl LedgerStore {
    /// Wraps a connection opened with [`open_ledger`](crate::open_ledger).
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Appends one trace.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn append_jev_trace(&mut self, trace: &JevTrace) -> Result<(), StoreError> {
        let body = serde_json::to_vec(trace).map_err(|_| corrupt("jev_traces.body"))?;
        self.conn.execute(
            "INSERT INTO jev_traces \
             (requested_secs, requested_nanos, request_hash, evidence_hash, body) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                trace.requested_at.unix_seconds(),
                trace.requested_at.subsec_nanos(),
                trace.request_hash.as_bytes().as_slice(),
                trace.evidence_hash.as_bytes().as_slice(),
                body
            ],
        )?;
        Ok(())
    }

    /// Every trace for the request with `hash`, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for a row that does not validate, or
    /// [`StoreError`] if SQLite fails.
    pub fn jev_traces_for_request(&self, hash: &PayloadHash) -> Result<Vec<JevTrace>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT requested_secs, requested_nanos, request_hash, evidence_hash, body \
             FROM jev_traces WHERE request_hash = ?1 ORDER BY seq",
        )?;
        let rows = query.query_map(params![hash.as_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        let mut traces = Vec::new();
        for row in rows {
            let (secs, nanos, request, evidence, body) = row?;
            let trace: JevTrace =
                serde_json::from_slice(&body).map_err(|_| corrupt("jev_traces.body"))?;
            let requested_at = Timestamp::from_unix(secs, nanos)
                .map_err(|_| corrupt("jev_traces.requested_secs"))?;
            let request: [u8; 32] = request
                .try_into()
                .map_err(|_| corrupt("jev_traces.request_hash"))?;
            let evidence: [u8; 32] = evidence
                .try_into()
                .map_err(|_| corrupt("jev_traces.evidence_hash"))?;
            if trace.requested_at != requested_at
                || trace.request_hash != PayloadHash::from_bytes(request)
                || trace.evidence_hash != EvidenceHash::from_bytes(evidence)
            {
                return Err(corrupt("jev_traces.body"));
            }
            traces.push(trace);
        }
        Ok(traces)
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        JevEffect, JudgeDescriptor, ModelId, PolicyVersion, SourceName, TraceValidation,
    };
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_ledger;

    fn trace(request: u8, seconds: i64) -> Result<JevTrace, Box<dyn std::error::Error>> {
        Ok(JevTrace {
            requested_at: Timestamp::from_unix(seconds, 500)?,
            judge: JudgeDescriptor {
                provider: SourceName::new("mock")?,
                model: ModelId::new("mock-1")?,
                model_version: None,
                prompt_version: "jev-judge/1".parse()?,
            },
            policy_version: PolicyVersion::new(0, 10, 0),
            input_schema: "jev.request/1".parse()?,
            output_schema: "jev.judgment/1".parse()?,
            evidence_hash: EvidenceHash::from_bytes([2; 32]),
            request_hash: PayloadHash::from_bytes([request; 32]),
            response_hash: None,
            validation: TraceValidation::Unavailable,
            judgment: None,
            calibrated_precision_bp: None,
            policy_effect: JevEffect::None,
            latency_ms: 25,
            note: None,
        })
    }

    fn ledger() -> Result<(Fixture, LedgerStore), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let store = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        Ok((fx, store))
    }

    #[test]
    fn traces_round_trip_by_request() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = ledger()?;
        let first = trace(4, 10)?;
        let second = trace(4, 20)?;
        store.append_jev_trace(&first)?;
        store.append_jev_trace(&trace(7, 15)?)?;
        store.append_jev_trace(&second)?;
        assert_eq!(
            store.jev_traces_for_request(&PayloadHash::from_bytes([4; 32]))?,
            [first, second]
        );
        assert_eq!(
            store
                .jev_traces_for_request(&PayloadHash::from_bytes([9; 32]))?
                .len(),
            0
        );
        Ok(())
    }

    #[test]
    fn the_ledger_is_append_only() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = ledger()?;
        store.append_jev_trace(&trace(4, 10)?)?;
        assert!(
            store
                .conn
                .execute("UPDATE jev_traces SET requested_secs = 0", [])
                .is_err()
        );
        assert!(store.conn.execute("DELETE FROM jev_traces", []).is_err());
        assert_eq!(
            store
                .jev_traces_for_request(&PayloadHash::from_bytes([4; 32]))?
                .len(),
            1
        );
        Ok(())
    }

    #[test]
    fn rows_that_disagree_with_their_body_are_reported() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, store) = ledger()?;
        let body = serde_json::to_vec(&trace(4, 10)?)?;
        store.conn.execute(
            "INSERT INTO jev_traces \
             (requested_secs, requested_nanos, request_hash, evidence_hash, body) \
             VALUES (10, 500, ?1, ?2, ?3)",
            params![[4_u8; 32].as_slice(), [3_u8; 32].as_slice(), body],
        )?;
        assert!(matches!(
            store.jev_traces_for_request(&PayloadHash::from_bytes([4; 32])),
            Err(StoreError::Corrupt { .. })
        ));
        Ok(())
    }
}
