//! `lumen jev key set | clear | status [--openrouter]`: the API key for the
//! judge: Jeb on the school AI Hub by default (ADR-0036), or `TypeSafe`'s Jev
//! through `OpenRouter` (roadmap 10.17, ADR-0031).
//!
//! The key is read from standard input, never from the command line (where
//! other processes can see it), checked, and kept in the operating system's
//! secret store: the macOS keychain or the Windows Credential Manager. It is
//! never printed. Storing a key sends nothing anywhere; Jev is consulted only
//! when a run asks for it explicitly (roadmap 10.17b).

use std::error::Error;

use lumen_application::ports::{Secret, SecretName, SecretStore};
use lumen_jev::openrouter::ApiKey;

use crate::jev_service::Service;

/// The platform's secret store (an error where there is none).
fn store() -> Result<Box<dyn SecretStore>, Box<dyn Error>> {
    lumen_app::ai::os_store()
        .ok_or_else(|| "no secret store on this platform yet; Jev is unavailable here".into())
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
/// `OpenRouter`. Without a stored key the run stops rather than silently
/// deciding without the judgment the user asked for.
pub(crate) fn judge() -> Result<lumen_jev::openrouter::JevJudge, Box<dyn Error>> {
    let (service, key) = stored_key()?;
    eprintln!("{}", service.disclosure());
    Ok(service.judge(key)?)
}

/// What turning AI review on will send, and where, for the stored key.
///
/// # Errors
///
/// If no key is stored: AI review cannot be on without one.
pub(crate) fn disclosure() -> Result<&'static str, Box<dyn Error>> {
    Ok(stored_key()?.0.disclosure())
}

fn stored_key() -> Result<(Service, ApiKey), Box<dyn Error>> {
    lumen_app::ai::stored_key(store()?.as_ref())?.ok_or_else(|| {
        "AI review needs an API key: run `lumen jev key set` (Jeb) or `lumen jev key set --openrouter` first".into()
    })
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

/// Which keys are stored (never the keys), and which one `--jev` uses.
pub(crate) fn status() -> Result<String, Box<dyn Error>> {
    let store = store()?;
    let jeb = store.get(SecretName::JEB_API_KEY)?.is_some();
    let openrouter = store.get(SecretName::OPENROUTER_API_KEY)?.is_some();
    let stored = |yes: bool| if yes { "stored" } else { "not stored" };
    let used = match (jeb, openrouter) {
        (true, _) => "--jev uses Jeb.",
        (false, true) => "--jev uses Jev through OpenRouter.",
        (false, false) => "--jev needs a key.",
    };
    Ok(format!(
        "Jeb API key: {}. OpenRouter API key: {}. {used}\n",
        stored(jeb),
        stored(openrouter)
    ))
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
}
