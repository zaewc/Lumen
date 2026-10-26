//! `lumen jev key set | clear | status [--openrouter]`: the API key for the
//! judge: Jeb on the school AI Hub by default (ADR-0036), or `TypeSafe`'s Jev
//! through `OpenRouter` (roadmap 10.17, ADR-0031).
//!
//! The key is read from standard input, never from the command line (where
//! other processes can see it), checked, and kept in the operating system's
//! secret store: the macOS keychain or the Windows Credential Manager. It is
//! never printed. Storing a key sends nothing anywhere; Jev is consulted only
//! when a run asks for it explicitly (roadmap 10.17b) or AI review is on
//! (ADR-0037).
//!
//! Release builds also carry Lumen's built-in Jeb key (ADR-0042), used only
//! when no key is stored. Status says whether it is available, never what it
//! is.

use std::error::Error;

use lumen_app::ai::{JudgeKey, KeySource};
use lumen_application::ports::{Secret, SecretName, SecretStore};
use lumen_jev::openrouter::ApiKey;

use crate::jev_service::Service;

/// Why keys cannot be stored or used where the platform has no secret store.
const NO_STORE: &str = "no secret store on this platform yet; Jev is unavailable here";

/// The platform's secret store (an error where there is none).
fn store() -> Result<Box<dyn SecretStore>, Box<dyn Error>> {
    lumen_app::ai::os_store().ok_or_else(|| NO_STORE.into())
}

/// Reads one key from `input`: the first line, trimmed. A key must be 1–256
/// visible ASCII characters.
pub(crate) fn read_key(input: &mut dyn std::io::BufRead) -> Result<String, Box<dyn Error>> {
    let mut line = String::new();
    input.read_line(&mut line)?;
    let key = line.trim();
    if ApiKey::new(key).is_none() {
        return Err(
            "that is not an API key (1-256 visible ASCII characters); nothing was stored".into(),
        );
    }
    Ok(key.to_owned())
}

/// Stores the key read from standard input for `service`.
pub(crate) fn set(service: Service) -> Result<String, Box<dyn Error>> {
    let key = read_key(&mut std::io::stdin().lock())?;
    store()?.set(service.secret(), &Secret::new(key))?;
    Ok(format!(
        "Stored the {} API key in the system's secret store. The judge is used only when a run passes --jev.\n",
        service.label()
    ))
}

/// The judge for a run that consults it, after telling the user what will be
/// sent and where: Jeb when its key is stored, otherwise Jev through
/// `OpenRouter`, otherwise Jeb with Lumen's built-in key (ADR-0042). Without
/// any key the run stops rather than silently deciding without the judgment
/// the user asked for.
pub(crate) fn judge() -> Result<lumen_jev::openrouter::JevJudge, Box<dyn Error>> {
    let key = judge_key()?;
    eprintln!("{}", key.disclosure());
    Ok(key.judge()?)
}

/// What turning AI review on will send, and where, with the key it would
/// use.
///
/// # Errors
///
/// If there is no key: AI review cannot be on without one.
pub(crate) fn disclosure() -> Result<&'static str, Box<dyn Error>> {
    Ok(judge_key()?.disclosure())
}

fn judge_key() -> Result<JudgeKey, Box<dyn Error>> {
    let store = lumen_app::ai::os_store();
    let builtin = lumen_app::ai::builtin_key();
    if store.is_none() && builtin.is_none() {
        return Err(NO_STORE.into());
    }
    lumen_app::ai::judge_key(store.as_deref(), builtin.as_ref())?.ok_or_else(|| {
        "AI review needs an API key: run `lumen jev key set` (Jeb) or `lumen jev key set --openrouter` first".into()
    })
}

/// Which key AI review would use, for `lumen ai status`; never the key.
pub(crate) fn key_line() -> Result<String, Box<dyn Error>> {
    let store = lumen_app::ai::os_store();
    let builtin = lumen_app::ai::builtin_key().is_some();
    Ok(
        match lumen_app::ai::key_source(store.as_deref(), builtin)? {
            Some((service, KeySource::Stored)) => format!(
                "Key: your {} key, from the system's secret store.\n",
                service.label()
            ),
            Some((_, KeySource::Builtin)) => {
                "Key: built-in key available; AI review uses Lumen's built-in Jeb key unless you store your own.\n".into()
            }
            None => "Key: none. Store one with `lumen jev key set`.\n".into(),
        },
    )
}

/// Records every Jev consultation of `assessed` in the ledger next to `db`.
pub(crate) fn record_traces(
    db: &std::path::Path,
    assessed: &[lumen_application::assess::Assessed],
) -> Result<(), Box<dyn Error>> {
    Ok(lumen_app::ai::record_traces(db, assessed)?)
}

/// Removes the stored key for `service`.
pub(crate) fn clear(service: Service) -> Result<String, Box<dyn Error>> {
    Ok(if store()?.delete(service.secret())? {
        format!("Removed the {} API key.\n", service.label())
    } else {
        format!("No {} API key was stored.\n", service.label())
    })
}

/// Which keys are stored and whether this build has a built-in key (never
/// the keys), and which one `--jev` uses.
pub(crate) fn status() -> Result<String, Box<dyn Error>> {
    let builtin = lumen_app::ai::builtin_key().is_some();
    let stored = match lumen_app::ai::os_store() {
        Some(store) => Some((
            store.contains(SecretName::JEB_API_KEY)?,
            store.contains(SecretName::OPENROUTER_API_KEY)?,
        )),
        None if builtin => None,
        None => return Err(NO_STORE.into()),
    };
    Ok(status_text(stored, builtin))
}

/// The status line for which keys are stored (`None`: no secret store) and
/// whether a built-in key is available.
fn status_text(stored: Option<(bool, bool)>, builtin: bool) -> String {
    let word = |yes: bool| if yes { "stored" } else { "not stored" };
    let (jeb, openrouter) = stored.unwrap_or_default();
    let used = match (jeb, openrouter, builtin) {
        (true, _, _) => "--jev uses your Jeb key.",
        (false, true, _) => "--jev uses Jev through OpenRouter.",
        (false, false, true) => "--jev uses Lumen's built-in Jeb key.",
        (false, false, false) => "--jev needs a key.",
    };
    let keys = match stored {
        Some(_) => format!(
            "Jeb API key: {}. OpenRouter API key: {}.",
            word(jeb),
            word(openrouter)
        ),
        None => "No secret store on this platform yet, so no key can be stored.".to_owned(),
    };
    let builtin = if builtin {
        "Built-in Jeb key: available."
    } else {
        "Built-in Jeb key: not in this build."
    };
    format!("{keys} {builtin} {used}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_read_from_the_first_line_and_checked() {
        let mut ok: &[u8] = b"sk-or-v1-abc123\nignored\n";
        assert!(read_key(&mut ok).is_ok());
        for bad in [&b"\n"[..], b"has space\n", b"tab\there\n", &[b'x'; 300]] {
            let mut input = bad;
            assert!(read_key(&mut input).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn status_says_which_key_is_used_never_the_key() {
        assert_eq!(
            status_text(Some((false, false)), false),
            "Jeb API key: not stored. OpenRouter API key: not stored. Built-in Jeb key: not in this build. --jev needs a key.\n"
        );
        assert_eq!(
            status_text(Some((false, false)), true),
            "Jeb API key: not stored. OpenRouter API key: not stored. Built-in Jeb key: available. --jev uses Lumen's built-in Jeb key.\n"
        );
        assert!(status_text(Some((true, false)), true).ends_with("--jev uses your Jeb key.\n"));
        assert!(
            status_text(Some((false, true)), true)
                .ends_with("--jev uses Jev through OpenRouter.\n")
        );
        assert!(status_text(None, true).starts_with("No secret store"));
    }
}
