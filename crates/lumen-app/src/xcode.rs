//! Explicit simulator-runtime metadata inventory. No image path is traversed.

pub use crate::inspection::InspectionError;
use crate::inspection::{Commands, Program};
use lumen_domain::UntrustedText;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A runtime disk image reported by Apple's `simctl`, not a cleanup candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Runtime {
    /// Escaped identifier for display; never accepted back as an action.
    pub identifier: String,
    /// OS version and build.
    pub version: String,
    /// Build identifier.
    pub build: String,
    /// Tool-reported image size; not physically reclaimable bytes.
    pub reported_image_bytes: Option<String>,
    /// Tool-reported state, without interpreting it as safe to remove.
    pub state: String,
}

/// Local metadata and the supported native management workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inventory {
    /// Version of this CLI result contract.
    pub schema_version: u32,
    /// The tool that owns these images.
    pub owner: &'static str,
    /// Runtime observations, including unavailable ones.
    pub runtimes: Vec<Runtime>,
    /// Fixed instruction; no deletion is offered by this adapter.
    pub management: &'static str,
    /// Official management documentation.
    pub documentation: &'static str,
    /// Why the reported byte count is not a cleanup promise.
    pub limitation: &'static str,
}

#[derive(Deserialize)]
struct Row {
    identifier: String,
    version: String,
    build: String,
    state: String,
    #[serde(rename = "sizeBytes")]
    size: Option<u64>,
}

/// Inspect the selected Xcode's runtime disk images on explicit request.
///
/// # Errors
/// [`InspectionError`] if unsupported, unavailable, incomplete or malformed.
pub fn inspect() -> Result<Inventory, InspectionError> {
    if !cfg!(target_os = "macos") {
        return Err(InspectionError::Unsupported);
    }
    inspect_with(&mut Program::find("xcrun")?)
}

fn inspect_with(commands: &mut impl Commands) -> Result<Inventory, InspectionError> {
    let output = commands.query(&["simctl", "runtime", "list", "--json"])?;
    let rows: BTreeMap<String, Row> = serde_json::from_str(&output)
        .map_err(|_| InspectionError::Format("simctl runtime list JSON"))?;
    let mut runtimes = Vec::new();
    for (id, row) in rows {
        if id != row.identifier
            || [id.as_str(), &row.version, &row.build, &row.state]
                .iter()
                .any(|value| value.is_empty() || value.len() > 256)
        {
            return Err(InspectionError::Format("simctl runtime identity or fields"));
        }
        runtimes.push(Runtime {
            identifier: UntrustedText::new(id).display(),
            version: UntrustedText::new(row.version).display(),
            build: UntrustedText::new(row.build).display(),
            reported_image_bytes: row.size.map(|bytes| bytes.to_string()),
            state: UntrustedText::new(row.state).display(),
        });
    }
    Ok(Inventory {
        schema_version: 1,
        owner: "Xcode",
        runtimes,
        management: "Review runtimes in Xcode > Settings > Components. Manage simulator user data in Window > Devices and Simulators.",
        documentation: "https://developer.apple.com/documentation/safari-developer-tools/adding-additional-simulators",
        limitation: "Image size is not reclaimable space. Archives and simulator user data may be valuable; nothing is removed by this inventory.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_metadata_is_review_only_and_paths_are_never_followed() -> Result<(), InspectionError>
    {
        let mut fake = |args: &[&str]| {
            assert_eq!(args, ["simctl", "runtime", "list", "--json"]);
            Ok(r#"{"A":{"identifier":"A","version":"26.1","build":"23B86","state":"Ready","sizeBytes":123,"path":"/private/do-not-open"},"B":{"identifier":"B","version":"25","build":"old","state":"Unavailable"}}"#.into())
        };
        let result = inspect_with(&mut fake)?;
        assert_eq!(result.runtimes.len(), 2);
        assert_eq!(
            result.runtimes[0].reported_image_bytes.as_deref(),
            Some("123")
        );
        assert_eq!(result.runtimes[1].reported_image_bytes, None);
        assert!(
            !serde_json::to_string(&result)
                .unwrap_or_default()
                .contains("do-not-open")
        );
        Ok(())
    }

    #[test]
    fn unavailable_and_unknown_formats_are_errors_not_zero_usage() {
        for output in [
            "[]",
            "null",
            r#"{"A":{"identifier":"other","version":"1","build":"B","state":"Ready"}}"#,
        ] {
            assert!(inspect_with(&mut |_: &[&str]| Ok(output.into())).is_err());
        }
        assert!(matches!(
            inspect_with(&mut |_: &[&str]| Err(InspectionError::Failed(Some(1)))),
            Err(InspectionError::Failed(Some(1)))
        ));
    }
}
