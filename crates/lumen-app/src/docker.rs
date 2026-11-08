//! Docker's own usage estimates, queried explicitly without filesystem access.

use crate::inspection::{Commands, InspectionError, Program};
use lumen_domain::UntrustedText;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Docker daemon storage observations, never a cleanup plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inventory {
    /// Version of the CLI result contract.
    pub schema_version: u32,
    /// Docker client version, for interpreting changing tool output.
    pub client_version: String,
    /// Context Docker selected, after its `DOCKER_HOST` / `DOCKER_CONTEXT` precedence.
    pub context: String,
    /// Effective daemon endpoint; may be remote.
    pub endpoint: String,
    /// Whether Docker reports rootless mode; unknown when not reported.
    pub rootless: Option<bool>,
    /// Images, containers, volumes and build cache, separately.
    pub categories: Vec<Usage>,
    /// Why unused resources cannot be assumed safe to remove.
    pub limitation: &'static str,
}

/// One complete row of `docker system df`. Sizes retain Docker's units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Usage {
    /// Docker's category name.
    pub kind: String,
    /// Resource counts, not file counts.
    pub total: u64,
    /// Active resources as Docker defines them.
    pub active: u64,
    /// Docker-reported size; neither physical bytes nor local free space.
    pub reported_size: String,
    /// Docker's estimate, without interpreting it as safe cleanup.
    pub reported_reclaimable: String,
    /// Every category requires review; no automatic prune is permitted here.
    pub review_required: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Row {
    #[serde(rename = "Type")]
    kind: String,
    total_count: String,
    active: String,
    size: String,
    reclaimable: String,
}

fn display(value: &str) -> Result<String, InspectionError> {
    if value.is_empty() || value.len() > 512 {
        return Err(InspectionError::Format("Docker field size"));
    }
    Ok(UntrustedText::new(value).display())
}

fn rows(output: &str) -> Result<Vec<Usage>, InspectionError> {
    let mut found = BTreeSet::new();
    let mut result = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)
            .map_err(|_| InspectionError::Format("docker system df JSON"))?;
        if !["Images", "Containers", "Local Volumes", "Build Cache"].contains(&row.kind.as_str())
            || !found.insert(row.kind.clone())
        {
            return Err(InspectionError::Format("Docker usage categories"));
        }
        let count = |text: &str| {
            text.parse::<u64>()
                .map_err(|_| InspectionError::Format("Docker resource count"))
        };
        let total = count(&row.total_count)?;
        let active = count(&row.active)?;
        if active > total {
            return Err(InspectionError::Format("Docker active count"));
        }
        result.push(Usage {
            kind: row.kind,
            total,
            active,
            reported_size: display(&row.size)?,
            reported_reclaimable: display(&row.reclaimable)?,
            review_required: true,
        });
    }
    if found.len() != 4 {
        return Err(InspectionError::Format(
            "incomplete Docker usage categories",
        ));
    }
    result.sort_by(|a, b| a.kind.cmp(&b.kind));
    Ok(result)
}

fn query(
    commands: &mut impl Commands,
    selector: &[String],
    args: &[&str],
) -> Result<String, InspectionError> {
    let args = selector
        .iter()
        .map(String::as_str)
        .chain(args.iter().copied())
        .collect::<Vec<_>>();
    commands.query(&args)
}

fn context(commands: &mut impl Commands) -> Result<(String, String), InspectionError> {
    let name = commands.query(&["context", "show"])?;
    let name = name.trim();
    // A leading dash would be parsed as an option by the next query.
    if name.starts_with('-') || name.chars().any(char::is_control) {
        return Err(InspectionError::Format("Docker context name"));
    }
    display(name)?;
    let raw = commands.query(&[
        "context",
        "inspect",
        name,
        "--format",
        "{{json .Endpoints.docker.Host}}",
    ])?;
    let endpoint: String = serde_json::from_str(&raw)
        .map_err(|_| InspectionError::Format("Docker context endpoint"))?;
    display(&endpoint)?;
    Ok((name.into(), endpoint))
}

/// Inspect the selected daemon only on explicit request. No cleanup is run.
///
/// # Errors
/// [`InspectionError`] for unavailable tools/daemons, malformed output, or
/// resources/context that change while the inventory is being read.
pub fn inspect() -> Result<Inventory, InspectionError> {
    inspect_with(&mut Program::find("docker")?)
}

fn inspect_with(commands: &mut impl Commands) -> Result<Inventory, InspectionError> {
    let version = commands.query(&["--version"])?;
    let version = version.trim();
    if !version.starts_with("Docker version ") {
        return Err(InspectionError::Format("Docker client version"));
    }
    // `context show` applies Docker's own DOCKER_HOST / DOCKER_CONTEXT precedence;
    // pinning its answer keeps every later query on the same daemon.
    let (name, endpoint) = context(commands)?;
    let selector = vec!["--context".to_owned(), name.clone()];
    let security = query(
        commands,
        &selector,
        &["info", "--format", "{{json .SecurityOptions}}"],
    )?;
    let security: Option<Vec<String>> = serde_json::from_str(&security)
        .map_err(|_| InspectionError::Format("Docker security options"))?;
    let rootless = security.map(|options| options.iter().any(|option| option == "name=rootless"));
    let args = ["system", "df", "--format", "json"];
    let categories = rows(&query(commands, &selector, &args)?)?;
    if categories != rows(&query(commands, &selector, &args)?)?
        || (name.clone(), endpoint.clone()) != context(commands)?
    {
        return Err(InspectionError::Changed("Docker context or resource usage"));
    }
    Ok(Inventory {
        schema_version: 1,
        client_version: display(version)?,
        context: display(&name)?,
        endpoint: display(&endpoint)?,
        rootless,
        categories,
        limitation: "Docker estimates belong to the selected daemon, which may be remote. Unused containers and volumes can contain valuable data. Review resources before a separately confirmed cleanup; this inventory runs no prune command.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage() -> String {
        ["Images", "Containers", "Local Volumes", "Build Cache"].into_iter().map(|kind|
            format!(r#"{{"Type":"{kind}","TotalCount":"2","Active":"1","Size":"5GB","Reclaimable":"1GB (20%)"}}"#))
            .collect::<Vec<_>>().join("\n")
    }

    fn fake(args: &[&str]) -> Result<String, InspectionError> {
        match args {
            ["--version"] => Ok("Docker version 29.0.0, build abc".into()),
            ["context", "show"] => Ok("work".into()),
            [
                "context",
                "inspect",
                "work",
                "--format",
                "{{json .Endpoints.docker.Host}}",
            ] => Ok(r#""unix:///run/user/1000/docker.sock""#.into()),
            [
                "--context",
                "work",
                "info",
                "--format",
                "{{json .SecurityOptions}}",
            ] => Ok(r#"["name=rootless"]"#.into()),
            ["--context", "work", "system", "df", "--format", "json"] => Ok(usage()),
            _ => Err(InspectionError::Format("unreviewed query")),
        }
    }

    #[test]
    fn inventories_all_categories_in_the_pinned_rootless_context() -> Result<(), InspectionError> {
        let result = inspect_with(&mut fake)?;
        assert_eq!(result.rootless, Some(true));
        assert_eq!(result.categories.len(), 4);
        assert!(result.categories.iter().all(|row| row.review_required));
        Ok(())
    }

    #[test]
    fn incomplete_malformed_or_duplicate_rows_are_not_zero_usage() {
        for output in [
            String::new(),
            "not JSON".into(),
            usage().lines().next().unwrap_or_default().into(),
            format!("{}\n{}", usage(), usage()),
            usage().replace("\"1\"", "\"9\""),
        ] {
            assert!(rows(&output).is_err());
        }
        assert!(matches!(
            inspect_with(&mut |_: &[&str]| Err(InspectionError::Failed(Some(13)))),
            Err(InspectionError::Failed(Some(13)))
        ));
    }

    #[test]
    fn option_like_contexts_and_a_missing_daemon_are_errors() {
        let mut dashed = |args: &[&str]| {
            if args == ["context", "show"] {
                Ok("--host=tcp://elsewhere".into())
            } else {
                fake(args)
            }
        };
        assert!(inspect_with(&mut dashed).is_err());
        let mut no_daemon = |args: &[&str]| {
            if args.contains(&"info") {
                Err(InspectionError::Failed(Some(1)))
            } else {
                fake(args)
            }
        };
        assert!(matches!(
            inspect_with(&mut no_daemon),
            Err(InspectionError::Failed(Some(1)))
        ));
    }

    #[test]
    fn changing_usage_or_context_requires_another_inspection() {
        let mut reads = 0;
        let mut changing = |args: &[&str]| {
            let output = fake(args)?;
            if args.contains(&"df") {
                reads += 1;
            }
            Ok(if reads == 2 {
                output.replace("5GB", "6GB")
            } else {
                output
            })
        };
        assert!(matches!(
            inspect_with(&mut changing),
            Err(InspectionError::Changed(_))
        ));
        let mut reads = 0;
        let mut changing = |args: &[&str]| {
            if args == ["context", "show"] {
                reads += 1;
            }
            if reads == 2 {
                Ok("other".into())
            } else {
                fake(args)
            }
        };
        assert!(inspect_with(&mut changing).is_err());
    }
}
