//! Explicit read-only tool queries. This command never executes cleanup.

use std::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
    json: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Args, String> {
    match args {
        [tool] if tool == "xcode" => Ok(Args { json: false }),
        [tool, flag] if tool == "xcode" && flag == "--json" => Ok(Args { json: true }),
        _ => Err("inspect needs xcode [--json]; inspection never accepts cleanup arguments".into()),
    }
}

pub(crate) fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let inventory = lumen_app::xcode::inspect()?;
    if args.json {
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
        for args in [
            vec!["xcode", "delete"],
            vec!["xcode", ";rm"],
            vec!["../tool"],
            vec![],
        ] {
            assert!(parse(&strings(&args)).is_err());
        }
    }
}
