//! Which app each uninstall moved into quarantine (ADR-0062): Lumen's own
//! record that an app with that bundle identifier existed and was removed, so
//! its data in `~/Library` can be recognised as left by a removed app
//! (ADR-0068 rule 1.2).

use lumen_domain::{OperationId, UntrustedText};
use rusqlite::params;

use super::LedgerStore;
use crate::StoreError;

/// An app an uninstall moved, as observed right before the move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstalledApp {
    /// The quarantine operation that moved its bundle.
    pub operation: OperationId,
    /// Its bundle identifier, as the bundle stated it.
    pub bundle_id: UntrustedText,
    /// Its display name, if it had one.
    pub name: Option<UntrustedText>,
}

impl LedgerStore {
    /// Records which app `operation` moved. A second record for the same
    /// operation is ignored: the first stands. An empty identifier, or one
    /// longer than 255 bytes, is not recorded: it names no app.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn record_uninstalled_app(
        &mut self,
        operation: OperationId,
        bundle_id: &UntrustedText,
        name: Option<&UntrustedText>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO uninstalled_apps (operation_id, bundle_id, name) \
             VALUES (?1, ?2, ?3)",
            params![
                operation.to_string(),
                bundle_id.raw(),
                name.map(UntrustedText::raw)
            ],
        )?;
        Ok(())
    }

    /// Every recorded uninstalled app, by operation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for an invalid row, or [`StoreError`]
    /// if SQLite fails.
    pub fn uninstalled_apps(&self) -> Result<Vec<UninstalledApp>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT operation_id, bundle_id, name FROM uninstalled_apps ORDER BY operation_id",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (operation, bundle_id, name) = row?;
            Ok(UninstalledApp {
                operation: operation
                    .parse()
                    .map_err(|_| StoreError::corrupt("uninstalled_apps.operation_id"))?,
                bundle_id: UntrustedText::new(bundle_id),
                name: name.map(UntrustedText::new),
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_ledger;

    fn operation(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f7081920{n}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn uninstalled_apps_are_recorded_once_and_never_changed()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let mut ledger = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        assert_eq!(ledger.uninstalled_apps()?, []);
        let notes = UntrustedText::new("net.example.Notes");
        let name = UntrustedText::new("Notes");
        ledger.record_uninstalled_app(operation(1), &notes, Some(&name))?;
        ledger.record_uninstalled_app(operation(1), &UntrustedText::new("other.id.x"), None)?;
        ledger.record_uninstalled_app(
            operation(2),
            &UntrustedText::new("org.example.Tool"),
            None,
        )?;
        assert_eq!(
            ledger.uninstalled_apps()?,
            [
                UninstalledApp {
                    operation: operation(1),
                    bundle_id: notes,
                    name: Some(name),
                },
                UninstalledApp {
                    operation: operation(2),
                    bundle_id: UntrustedText::new("org.example.Tool"),
                    name: None,
                },
            ]
        );
        let tampered = ledger
            .conn
            .execute("UPDATE uninstalled_apps SET bundle_id = 'x'", []);
        assert!(tampered.is_err(), "append-only");
        ledger.record_uninstalled_app(operation(3), &UntrustedText::new(""), None)?;
        assert_eq!(
            ledger.uninstalled_apps()?.len(),
            2,
            "an empty identifier names no app"
        );
        Ok(())
    }
}
