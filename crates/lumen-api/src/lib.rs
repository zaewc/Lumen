//! Lumen's local API contract (ADR-0011, roadmap 11.5).
//!
//! The view models in [`views`] are what the desktop UI (over Tauri IPC) and
//! the optional browser dashboard (over loopback HTTP) read and send. Their
//! `OpenAPI` 3.1 description, [`openapi`], is exported by `cargo xtask schema`
//! to `schemas/openapi.json`, and the TypeScript client is generated from
//! that file, so a change to these types shows up as a reviewable diff.

pub mod views;

use utoipa::OpenApi;

/// The API description.
#[derive(Debug, OpenApi)]
#[openapi(
    info(
        title = "Lumen local API",
        description = "View models for Lumen's desktop UI and browser dashboard (ADR-0011).",
        license(name = "Apache-2.0", identifier = "Apache-2.0"),
    ),
    components(schemas(
        views::ScanRequest,
        views::ScanView,
        views::ScanProgressView,
        views::RootView,
        views::VerdictView,
        views::CandidateView,
        views::ReasonView,
        views::ToolCommandView,
        views::PlanView,
        views::ExecuteRequest,
        views::LocationView,
        views::SkipView,
        views::ItemResultView,
        views::ItemReportView,
        views::QuarantineReportView,
        views::HeldStateView,
        views::HeldItemView,
        views::NotRestoredView,
        views::RestoreView,
        views::RestoreOutcomeView,
        views::RemovedView,
        views::FinalizeView,
        views::FinalizeOutcomeView,
    ))
)]
pub struct ApiDoc;

/// The `OpenAPI` document, as committed in `schemas/openapi.json`.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use lumen_domain::{ByteCount, ItemLocation, QuarantineFailure};

    use super::*;

    #[test]
    fn the_document_is_openapi_3_1_with_every_view() {
        let doc = serde_json::to_value(openapi()).unwrap_or_default();
        assert!(
            doc["openapi"]
                .as_str()
                .is_some_and(|v| v.starts_with("3.1")),
            "{}",
            doc["openapi"]
        );
        let schemas = doc["components"]["schemas"]
            .as_object()
            .map(serde_json::Map::len)
            .unwrap_or_default();
        assert_eq!(schemas, 23);
    }

    #[test]
    fn counts_are_strings_and_outcomes_are_tagged() {
        let report = lumen_app::quarantine::QuarantineReport {
            plan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
                .parse()
                .unwrap_or_else(|_| unreachable!()),
            resolved: 0,
            items: vec![lumen_app::quarantine::ItemReport {
                path: lumen_domain::RawPath::from_unix_bytes(b"/home/t/x".to_vec())
                    .unwrap_or_else(|_| unreachable!()),
                reclaim: ByteCount::new(u64::MAX),
                result: lumen_app::quarantine::ItemResult::Failed {
                    reason: QuarantineFailure::IdentityMismatch,
                    left: ItemLocation::Source,
                },
            }],
            moved: ByteCount::ZERO,
            warnings: Vec::new(),
        };
        let json =
            serde_json::to_value(views::QuarantineReportView::from(&report)).unwrap_or_default();
        assert_eq!(
            json["items"][0]["reclaimable_bytes"],
            u64::MAX.to_string(),
            "no precision lost in JavaScript"
        );
        assert_eq!(json["items"][0]["result"]["kind"], "failed");
        assert_eq!(json["items"][0]["result"]["left"], "source");
        assert_eq!(json["moved_bytes"], "0");
    }

    #[test]
    fn absent_values_are_required_and_null() {
        let doc = serde_json::to_value(openapi()).unwrap_or_default();
        let held = &doc["components"]["schemas"]["HeldItemView"];
        let required: Vec<&str> = held["required"]
            .as_array()
            .map(|r| r.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        assert!(required.contains(&"original"), "{required:?}");
        let candidate = &doc["components"]["schemas"]["CandidateView"];
        let required: Vec<&str> = candidate["required"]
            .as_array()
            .map(|r| r.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        assert!(required.contains(&"tool_command"), "{required:?}");
    }
}
