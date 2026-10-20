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

fn secret(service: Service) -> SecretName {
    match service {
        Service::Jeb => SecretName::JEB_API_KEY,
        Service::OpenRouter => SecretName::OPENROUTER_API_KEY,
    }
}

const fn label(service: Service) -> &'static str {
    match service {
        Service::Jeb => "Jeb",
        Service::OpenRouter => "OpenRouter",
    }
}

/// The platform's secret store (an error where there is none).
#[cfg_attr(
    any(target_os = "macos", windows),
    expect(
        clippy::unnecessary_wraps,
        reason = "fails on platforms without a store"
    )
)]
fn store() -> Result<Box<dyn SecretStore>, Box<dyn Error>> {
    #[cfg(target_os = "macos")]
    return Ok(Box::new(lumen_platform_macos::KeychainStore::new()));
    #[cfg(windows)]
    return Ok(Box::new(lumen_platform_windows::CredentialStore::new()));
    #[cfg(not(any(target_os = "macos", windows)))]
    Err("no secret store on this platform yet; Jev is unavailable here".into())
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
    store()?.set(secret(service), &Secret::new(key))?;
    Ok(format!(
        "Stored the {} API key in the system's secret store. The judge is used only when a run passes --jev.\n",
        label(service)
    ))
}

/// The judge for a run that consults it, after telling the user what will be
/// sent and where: Jeb when its key is stored, otherwise Jev through
/// `OpenRouter`. Without a stored key the run stops rather than silently
/// deciding without the judgment the user asked for.
pub(crate) fn judge() -> Result<lumen_jev::openrouter::JevJudge, Box<dyn Error>> {
    let (service, key) = stored_key()?;
    eprintln!("{}", disclosure_for(service));
    Ok(match service {
        Service::Jeb => lumen_jev::openrouter::JevJudge::jeb(key)?,
        Service::OpenRouter => lumen_jev::openrouter::JevJudge::new(key)?,
    })
}

/// What turning AI review on will send, and where, for the stored key.
///
/// # Errors
///
/// If no key is stored: AI review cannot be on without one.
pub(crate) fn disclosure() -> Result<&'static str, Box<dyn Error>> {
    Ok(disclosure_for(stored_key()?.0))
}

fn stored_key() -> Result<(Service, ApiKey), Box<dyn Error>> {
    let store = store()?;
    for service in [Service::Jeb, Service::OpenRouter] {
        if let Some(secret) = store.get(secret(service))? {
            let key = ApiKey::new(secret.expose())
                .ok_or_else(|| format!("the stored {} API key is not valid", label(service)))?;
            return Ok((service, key));
        }
    }
    Err("AI review needs an API key: run `lumen jev key set` (Jeb) or `lumen jev key set --openrouter` first".into())
}

const fn disclosure_for(service: Service) -> &'static str {
    match service {
        Service::Jeb => {
            "Jeb: for each item proposed for quarantine, its metadata (path with your home as ~, sizes, ages, evidence facts; never file contents) goes to the school's Jeb server (jeb.https.gsmsv.site) with your key. Its operator may keep requests. Jeb can only make a decision more cautious."
        }
        Service::OpenRouter => {
            "Jev: for each item proposed for quarantine, its metadata (path with your home as ~, sizes, ages, evidence facts; never file contents) goes to TypeSafe's Jev through OpenRouter with your key. Jev can only make a decision more cautious."
        }
    }
}

/// Records every Jev consultation of `assessed` in the ledger next to `db`.
pub(crate) fn record_traces(
    db: &std::path::Path,
    assessed: &[lumen_application::assess::Assessed],
) -> Result<(), Box<dyn Error>> {
    let traces: Vec<_> = assessed.iter().filter_map(|a| a.trace.as_ref()).collect();
    if traces.is_empty() {
        return Ok(());
    }
    let mut ledger = lumen_store_sqlite::LedgerStore::new(lumen_store_sqlite::open_ledger(
        &db.with_file_name("ledger.db"),
    )?);
    for trace in traces {
        ledger.append_jev_trace(trace)?;
    }
    Ok(())
}

/// Removes the stored key for `service`.
pub(crate) fn clear(service: Service) -> Result<String, Box<dyn Error>> {
    Ok(if store()?.delete(secret(service))? {
        format!("Removed the {} API key.\n", label(service))
    } else {
        format!("No {} API key was stored.\n", label(service))
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
