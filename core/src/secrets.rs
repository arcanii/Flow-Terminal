//! API key storage. Keys live in the OS credential store (Windows Credential
//! Manager, macOS Keychain), one per provider; the provider's variable in the
//! environment (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`) wins
//! if set. The bearer tokens of the MCP servers the assistant uses are kept
//! there too, one per server, and their sign-ins, split over several
//! entries where they're long (`store_long`).

use crate::ai::Provider;
use crate::APP_ID;

/// The credential store's account for a provider's key.
fn account(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "anthropic-api-key",
        Provider::OpenAi => "openai-api-key",
        Provider::Gemini => "gemini-api-key",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Environment,
    Keychain,
}

pub fn load_api_key(provider: Provider) -> Option<(String, KeySource)> {
    if let Ok(key) = std::env::var(provider.key_env()) {
        let key = key.trim().to_string();
        if !key.is_empty() {
            return Some((key, KeySource::Environment));
        }
    }
    let entry = keyring::Entry::new(APP_ID, account(provider)).ok()?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Some((key.trim().to_string(), KeySource::Keychain)),
        Ok(_) => None,
        Err(err) => {
            log::debug!("no stored API key: {err}");
            None
        }
    }
}

pub fn store_api_key(provider: Provider, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(APP_ID, account(provider)).map_err(|e| e.to_string())?;
    entry.set_password(key.trim()).map_err(|e| e.to_string())
}

pub fn forget_api_key(provider: Provider) -> Result<(), String> {
    let entry = keyring::Entry::new(APP_ID, account(provider)).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

/// The credential store's account for an MCP server's bearer token
/// (`crate::mcp::client`), kept by the server's name.
fn server_account(server: &str) -> String {
    format!("mcp-{server}")
}

/// The token kept for MCP server `server`, where there is one.
pub fn load_server_token(server: &str) -> Option<String> {
    let entry = keyring::Entry::new(APP_ID, &server_account(server)).ok()?;
    match entry.get_password() {
        Ok(token) if !token.trim().is_empty() => Some(token.trim().to_string()),
        Ok(_) => None,
        Err(err) => {
            log::debug!("no stored token for {server}: {err}");
            None
        }
    }
}

pub fn store_server_token(server: &str, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(APP_ID, &server_account(server)).map_err(|e| e.to_string())?;
    entry.set_password(token.trim()).map_err(|e| e.to_string())
}

pub fn forget_server_token(server: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(APP_ID, &server_account(server)).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

/// How much of a long secret goes in each of the credential store's
/// entries: Windows' Credential Manager holds 2,560 bytes an entry, kept
/// as UTF-16, so a sign-in's tokens are split over several.
const PIECE: usize = 1000;
/// At most so many pieces are read, written or forgotten.
const MOST_PIECES: usize = 32;

/// The account of a long secret's piece `n`: its own name for the first.
fn piece(account: &str, n: usize) -> String {
    if n == 0 {
        account.to_string()
    } else {
        format!("{account}.{}", n + 1)
    }
}

/// A long secret kept under `account`, put back together from its pieces.
pub fn load_long(account: &str) -> Option<String> {
    let mut text = String::new();
    for n in 0..MOST_PIECES {
        let entry = keyring::Entry::new(APP_ID, &piece(account, n)).ok()?;
        match entry.get_password() {
            Ok(part) => text.push_str(&part),
            Err(keyring::Error::NoEntry) => break,
            Err(err) => {
                log::debug!("no stored {account}: {err}");
                return None;
            }
        }
    }
    (!text.is_empty()).then_some(text)
}

/// Keeps `text` under `account`, in pieces where it's long, the pieces of
/// a longer one before taken away.
pub fn store_long(account: &str, text: &str) -> Result<(), String> {
    let chars: Vec<char> = text.chars().collect();
    let pieces: Vec<String> = chars.chunks(PIECE).map(|c| c.iter().collect()).collect();
    if pieces.len() > MOST_PIECES {
        return Err(format!("{account} is too long to keep"));
    }
    for (n, part) in pieces.iter().enumerate() {
        let entry = keyring::Entry::new(APP_ID, &piece(account, n)).map_err(|e| e.to_string())?;
        entry.set_password(part).map_err(|e| e.to_string())?;
    }
    forget_pieces(account, pieces.len())
}

/// Forgets a long secret, every piece of it.
pub fn forget_long(account: &str) -> Result<(), String> {
    forget_pieces(account, 0)
}

fn forget_pieces(account: &str, from: usize) -> Result<(), String> {
    for n in from..MOST_PIECES {
        let entry = keyring::Entry::new(APP_ID, &piece(account, n)).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => {}
            Err(keyring::Error::NoEntry) => break,
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(())
}

/// `sk-ant-api03-abcd…wxyz` style preview for display.
pub fn mask(key: &str) -> String {
    let key = key.trim();
    if key.len() <= 16 {
        return "•".repeat(key.len());
    }
    format!("{}…{}", &key[..12], &key[key.len() - 4..])
}
