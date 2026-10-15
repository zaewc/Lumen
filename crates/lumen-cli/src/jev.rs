//! `lumen jev key set | clear | status`: the `OpenRouter` API key for Jev
//! (roadmap 10.17, ADR-0031).
//!
//! The key is read from standard input, never from the command line (where
//! other processes can see it), checked, and kept in the operating system's
//! secret store: the macOS keychain or the Windows Credential Manager. It is
//! never printed. Storing a key sends nothing anywhere; Jev is consulted only
//! when a run asks for it explicitly (roadmap 10.17b).

use std::error::Error;

use lumen_application::ports::{Secret, SecretName, SecretStore};
use lumen_jev::openrouter::ApiKey;

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
            "that is not an OpenRouter API key (1-256 visible ASCII characters); nothing was stored"
                .into(),
        );
    }
    Ok(key.to_owned())
}

/// Stores the key read from standard input.
pub(crate) fn set() -> Result<String, Box<dyn Error>> {
    let key = read_key(&mut std::io::stdin().lock())?;
    store()?.set(SecretName::OPENROUTER_API_KEY, &Secret::new(key))?;
    Ok("Stored the OpenRouter API key in the system's secret store. Jev is used only when a run passes --jev.\n".into())
}

/// The judge for a run that passed `--jev`: Jev with the stored key, after
/// telling the user what will be sent. Without a stored key the run stops
/// rather than silently deciding without the judgment the user asked for.
pub(crate) fn judge() -> Result<lumen_jev::openrouter::JevJudge, Box<dyn Error>> {
    let secret = store()?
        .get(SecretName::OPENROUTER_API_KEY)?
        .ok_or("--jev needs an OpenRouter API key: run `lumen jev key set` first")?;
    let key = ApiKey::new(secret.expose()).ok_or("the stored OpenRouter API key is not valid")?;
    eprintln!(
        "Jev: for each candidate the policy does not keep, its metadata (path with your home as ~, sizes, ages, evidence facts; never file contents) goes to TypeSafe's Jev through OpenRouter with your key. Jev can only make a decision more cautious."
    );
    Ok(lumen_jev::openrouter::JevJudge::new(key)?)
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

/// Removes the stored key.
pub(crate) fn clear() -> Result<String, Box<dyn Error>> {
    Ok(if store()?.delete(SecretName::OPENROUTER_API_KEY)? {
        "Removed the OpenRouter API key.\n".into()
    } else {
        "No OpenRouter API key was stored.\n".into()
    })
}

/// Whether a key is stored (never the key).
pub(crate) fn status() -> Result<String, Box<dyn Error>> {
    Ok(match store()?.get(SecretName::OPENROUTER_API_KEY)? {
        Some(_) => "An OpenRouter API key is stored.\n".into(),
        None => "No OpenRouter API key is stored.\n".into(),
    })
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
