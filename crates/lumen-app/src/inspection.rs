//! Bounded queries for explicit developer-tool inspection, never scanning.

use crate::tools::{self, ToolError, ToolRun};
use std::path::PathBuf;
use std::time::Duration;

/// Why an explicit tool inventory is unavailable. Missing data is never empty.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InspectionError {
    /// Tool discovery or process execution failed.
    #[error(transparent)]
    Tool(#[from] ToolError),
    /// The tool did not complete its query.
    #[error(
        "the inventory command failed (exit status {0:?}); check the tool's availability and permissions"
    )]
    Failed(Option<i32>),
    /// The query exceeded its time budget.
    #[error("the inventory command exceeded its 30 second time limit")]
    Timeout,
    /// A bounded result cannot be accepted as complete.
    #[error("the inventory output exceeded its size limit; no partial inventory was accepted")]
    OutputLimit,
    /// The installed tool uses an unsupported or malformed output format.
    #[error("unsupported tool output: {0}")]
    Format(&'static str),
    /// The tool's state changed between reads; nothing partial is reported.
    #[error("{0} changed during inspection")]
    Changed(&'static str),
    /// The adapter is not available on this OS.
    #[error("this inventory is not supported on this platform")]
    Unsupported,
}

pub(crate) trait Commands {
    fn query(&mut self, args: &[&str]) -> Result<String, InspectionError>;
}

pub(crate) struct Program {
    path: PathBuf,
    home: PathBuf,
}

impl Program {
    /// A system program at its fixed absolute `path`, never one found on the
    /// user's search path.
    pub(crate) fn system(path: &str) -> Result<Self, InspectionError> {
        let home = std::env::home_dir().ok_or(ToolError::NoHome)?;
        Ok(Self {
            path: PathBuf::from(path),
            home,
        })
    }

    pub(crate) fn find(name: &str) -> Result<Self, InspectionError> {
        let home = std::env::home_dir().ok_or(ToolError::NoHome)?;
        let path = tools::find_in(name, &tools::search_dirs(&home))
            .ok_or_else(|| ToolError::NotFound(name.into()))?;
        Ok(Self { path, home })
    }
}

impl Commands for Program {
    fn query(&mut self, args: &[&str]) -> Result<String, InspectionError> {
        let args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
        complete(tools::inspect_program(
            &self.path,
            &args,
            &self.home,
            Duration::from_secs(30),
        )?)
    }
}

fn complete(run: ToolRun) -> Result<String, InspectionError> {
    if run.timed_out {
        return Err(InspectionError::Timeout);
    }
    if !run.success {
        return Err(InspectionError::Failed(run.exit_code));
    }
    // The runner keeps a bounded tail; reject a full buffer rather than risk
    // accepting a valid-looking suffix as the whole response.
    if run.output.len() >= tools::OUTPUT_LIMIT {
        return Err(InspectionError::OutputLimit);
    }
    Ok(run.output)
}

#[cfg(test)]
impl<F> Commands for F
where
    F: FnMut(&[&str]) -> Result<String, InspectionError>,
{
    fn query(&mut self, args: &[&str]) -> Result<String, InspectionError> {
        self(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_timed_out_and_truncated_queries_are_never_empty_inventories() {
        let good = || ToolRun {
            success: true,
            exit_code: Some(0),
            timed_out: false,
            output: "{}".into(),
            space_changes: None,
        };
        let mut run = good();
        run.timed_out = true;
        assert!(matches!(complete(run), Err(InspectionError::Timeout)));
        let mut run = good();
        run.success = false;
        run.exit_code = Some(13);
        assert!(matches!(
            complete(run),
            Err(InspectionError::Failed(Some(13)))
        ));
        let mut run = good();
        run.output = "x".repeat(tools::OUTPUT_LIMIT);
        assert!(matches!(complete(run), Err(InspectionError::OutputLimit)));
        assert_eq!(complete(good()).ok(), Some("{}".into()));
    }
}
