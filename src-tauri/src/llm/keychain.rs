//! API key storage in the OS keychain (Windows Credential Manager / macOS
//! Keychain). The key is never written to settings.json or logs.
//!
//! The key is cached in memory after the first read so dictations don't pay
//! a keychain round-trip (and macOS doesn't re-prompt) every time.

use parking_lot::Mutex;

use super::LlmError;

const SERVICE: &str = "app.murmr.desktop";
const ACCOUNT: &str = "anthropic_api_key";

/// `None` = not loaded yet; `Some(None)` = loaded, no key saved.
static CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

fn entry() -> Result<keyring::Entry, LlmError> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| LlmError::Keychain(e.to_string()))
}

/// The saved Anthropic API key, if any.
pub fn get_key() -> Result<Option<String>, LlmError> {
    if let Some(cached) = CACHE.lock().as_ref() {
        return Ok(cached.clone());
    }
    let loaded = match entry()?.get_password() {
        Ok(k) if !k.trim().is_empty() => Some(k),
        Ok(_) | Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(LlmError::Keychain(e.to_string())),
    };
    *CACHE.lock() = Some(loaded.clone());
    Ok(loaded)
}

pub fn save_key(key: &str) -> Result<(), LlmError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(LlmError::Keychain("key is empty".into()));
    }
    entry()?
        .set_password(key)
        .map_err(|e| LlmError::Keychain(e.to_string()))?;
    *CACHE.lock() = Some(Some(key.to_string()));
    Ok(())
}

pub fn remove_key() -> Result<(), LlmError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(LlmError::Keychain(e.to_string())),
    }
    *CACHE.lock() = Some(None);
    Ok(())
}

/// "sk-ant-…abcd" style hint for the UI. Never returns the full key.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "••••".into();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    let head: String = chars.iter().take(7).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn mask_hides_middle() {
        assert_eq!(mask("sk-ant-api03-abcdefghijklmnop-WXYZ"), "sk-ant-…WXYZ");
        assert_eq!(mask("short"), "••••");
    }
}
