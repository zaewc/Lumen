//! Explicit read-only tool queries. This command never executes cleanup.

use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Xcode,
    Docker,
    Packages,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
    tool: Tool,
    json: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Args, String> {
    let tool = match args.first().map(String::as_str) {
        Some("xcode") => Tool::Xcode,
        Some("docker") => Tool::Docker,
        Some("packages") => Tool::Packages,
        _ => return Err(USAGE.into()),
    };
    match &args[1..] {
        [] => Ok(Args { tool, json: false }),
        [flag] if flag == "--json" => Ok(Args { tool, json: true }),
        _ => Err(USAGE.into()),
    }
}

const USAGE: &str =
    "inspect needs xcode|docker|packages [--json]; inspection never accepts cleanup arguments";

pub(crate) fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    match args.tool {
        Tool::Xcode => xcode(args.json),
        Tool::Docker => docker(args.json),
        Tool::Packages => packages(args.json),
    }
}

fn packages(json: bool) -> Result<(), Box<dyn Error>> {
    use lumen_app::packages::{Status, StoreKind};
    let inventory = lumen_app::packages::inspect();
    if json {
        println!("{}", serde_json::to_string(&inventory)?);
        return Ok(());
    }
    println!("Package manager locations (read-only)");
    for tool in &inventory.tools {
        let kind = match tool.kind {
            StoreKind::Cache => "cache",
            StoreKind::SharedStore => "shared store",
        };
        match &tool.status {
            Status::NotInstalled => println!("{}: not installed", tool.name),
            Status::Unavailable { reason } => println!("{}: unknown ({reason})", tool.name),
            Status::Found { version, location } => {
                // Homebrew prints "Homebrew 7.0.12"; the others a bare number.
                let version = version
                    .strip_prefix(tool.name)
                    .map_or(version.as_str(), str::trim);
                println!("{} {version}: {kind} at {location}", tool.name);
                if let Some(cleanup) = &tool.cleanup {
                    println!("  its own cleanup: {cleanup}");
                }
            }
        }
    }
    println!("{}", inventory.limitation);
    Ok(())
}

fn docker(json: bool) -> Result<(), Box<dyn Error>> {
    let inventory = lumen_app::docker::inspect()?;
    if json {
        println!("{}", serde_json::to_string(&inventory)?);
    } else {
        println!(
            "Docker storage inventory (read-only), {} at {}",
            inventory.context, inventory.endpoint
        );
        for usage in &inventory.categories {
            println!(
                "{}: {} total, {} active, {} reported, {} reported reclaimable",
                usage.kind,
                usage.total,
                usage.active,
                usage.reported_size,
                usage.reported_reclaimable
            );
        }
        println!("{}", inventory.limitation);
    }
    Ok(())
}

fn xcode(json: bool) -> Result<(), Box<dyn Error>> {
    let inventory = lumen_app::xcode::inspect()?;
    if json {
        println!("{}", serde_json::to_string(&inventory)?);
    } else {
        println!("Xcode runtime inventory (read-only)");
        for runtime in &inventory.runtimes {
            println!(
                "{} {}: {} image bytes, {}",
                runtime.version,
                runtime.build,
                runtime.reported_image_bytes.as_deref().unwrap_or("unknown"),
                runtime.state
            );
        }
        println!(
            "{}\n{}\n{}",
            inventory.management, inventory.documentation, inventory.limitation
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_accepts_no_cleanup_or_arbitrary_command_arguments() {
        let strings = |args: &[&str]| args.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(parse(&strings(&["xcode", "--json"])).is_ok());
        assert_eq!(
            parse(&strings(&["docker"])),
            Ok(Args {
                tool: Tool::Docker,
                json: false
            })
        );
        for args in [
            vec!["xcode", "delete"],
            vec!["docker", "system", "prune"],
            vec!["docker", "--json", "--force"],
            vec!["xcode", ";rm"],
            vec!["../tool"],
            vec![],
        ] {
            assert!(parse(&strings(&args)).is_err());
        }
    }
}
