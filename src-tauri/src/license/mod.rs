//! License key validation.
//!
//! Each license key is a compact `<payload_b64>.<signature_b64>` blob:
//!
//!   eyJlbWFpbCI6ImpvbkBleGFtcGxlLmNvbSIsImV4cGlyZXNfYXQiOm51bGx9.MEUCIQ…
//!
//! - `payload` is a JSON object: `{email, issued_at, expires_at?}`
//! - `signature` is an Ed25519 signature over the raw payload bytes
//! - both halves are base64url-encoded (no padding) so the key is one
//!   line, copy-pasteable, and URL-safe
//!
//! The app embeds the matching Ed25519 public key in `LICENSE_PUBLIC_KEY`
//! (set at compile time via `MURMR_LICENSE_PUBKEY` env var, falling back
//! to a placeholder dev key). The private key never ships and lives in
//! `.keys/license-priv.key` (gitignored). A Node CLI in
//! `scripts/issue-license.mjs` mints new keys.
//!
//! Why Ed25519: short keys (32 bytes), short signatures (64 bytes), no
//! parameter ambiguity, fast verify, well-supported in both Rust
//! (`ed25519-dalek`) and Node (`crypto.sign('ed25519')`).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
use serde::{Deserialize, Serialize};

/// Compile-time-baked Ed25519 public key (32 bytes, base64url-no-pad).
///
/// Set via `MURMR_LICENSE_PUBKEY` env var at build time; if unset, falls
/// back to an all-zero placeholder that rejects EVERY key (so nobody can
/// accidentally ship an unsigned-by-anyone build).
const LICENSE_PUBLIC_KEY_B64: &str = match option_env!("MURMR_LICENSE_PUBKEY") {
    Some(k) => k,
    None => "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    /// Identifies the licensee (for support / lookup; not validated).
    pub email: String,
    /// ISO 8601 timestamp the license was minted.
    pub issued_at: String,
    /// ISO 8601 timestamp after which the license is no longer valid.
    /// `None` = never expires.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Optional product / tier identifier so we can mint distinct license
    /// types in the future without breaking older installs.
    #[serde(default)]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LicenseStatus {
    /// No key was provided.
    Missing,
    /// The key is malformed (wrong shape, bad base64).
    Malformed { reason: String },
    /// Signature didn't verify against our public key.
    BadSignature,
    /// Signature verified but the embedded `expires_at` is in the past.
    Expired { email: String, expired_at: String },
    /// Valid, in-date, signature OK.
    Valid {
        email: String,
        tier: Option<String>,
        expires_at: Option<String>,
    },
}

impl LicenseStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, LicenseStatus::Valid { .. })
    }
}

/// Parse + verify a license key. Returns the structured status — UI can
/// branch on it to decide what to show (paywall, expired warning, OK).
///
/// `now_iso` is the current time as ISO 8601, injected so tests can pin
/// it. In production callers pass `chrono::Utc::now()` formatted, but
/// since we want to avoid a chrono dep we let the caller stringify
/// whatever wall clock they already have.
pub fn validate(key: &str, now_iso: &str) -> LicenseStatus {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return LicenseStatus::Missing;
    }

    let (payload_b64, sig_b64) = match trimmed.split_once('.') {
        Some(parts) => parts,
        None => {
            return LicenseStatus::Malformed {
                reason: "expected '<payload>.<signature>' format".into(),
            }
        }
    };

    let payload_bytes = match URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(b) => b,
        Err(e) => {
            return LicenseStatus::Malformed {
                reason: format!("payload base64 decode: {e}"),
            }
        }
    };
    let sig_bytes = match URL_SAFE_NO_PAD.decode(sig_b64) {
        Ok(b) => b,
        Err(e) => {
            return LicenseStatus::Malformed {
                reason: format!("signature base64 decode: {e}"),
            }
        }
    };

    if sig_bytes.len() != SIGNATURE_LENGTH {
        return LicenseStatus::Malformed {
            reason: format!(
                "signature is {} bytes (expected {})",
                sig_bytes.len(),
                SIGNATURE_LENGTH
            ),
        };
    }

    let pubkey_bytes = match URL_SAFE_NO_PAD.decode(LICENSE_PUBLIC_KEY_B64) {
        Ok(b) => b,
        Err(_) => {
            return LicenseStatus::Malformed {
                reason: "build-time license pubkey is not valid base64".into(),
            }
        }
    };
    if pubkey_bytes.len() != PUBLIC_KEY_LENGTH {
        return LicenseStatus::Malformed {
            reason: format!(
                "build-time license pubkey is {} bytes (expected {})",
                pubkey_bytes.len(),
                PUBLIC_KEY_LENGTH
            ),
        };
    }
    // Reject the all-zero placeholder explicitly so a missing build env
    // var doesn't silently produce signature failures the user can't
    // diagnose.
    if pubkey_bytes.iter().all(|&b| b == 0) {
        return LicenseStatus::Malformed {
            reason: "this build was compiled without MURMR_LICENSE_PUBKEY — no key can validate".into(),
        };
    }

    let pubkey_array: [u8; PUBLIC_KEY_LENGTH] = pubkey_bytes.try_into().unwrap();
    let verifying_key = match VerifyingKey::from_bytes(&pubkey_array) {
        Ok(k) => k,
        Err(e) => {
            return LicenseStatus::Malformed {
                reason: format!("pubkey not on Ed25519 curve: {e}"),
            }
        }
    };

    let sig_array: [u8; SIGNATURE_LENGTH] = sig_bytes.try_into().unwrap();
    let signature = Signature::from_bytes(&sig_array);

    if verifying_key.verify(&payload_bytes, &signature).is_err() {
        return LicenseStatus::BadSignature;
    }

    let payload: LicensePayload = match serde_json::from_slice(&payload_bytes) {
        Ok(p) => p,
        Err(e) => {
            return LicenseStatus::Malformed {
                reason: format!("payload JSON parse: {e}"),
            }
        }
    };

    if let Some(expires_at) = &payload.expires_at {
        // Lexicographic compare on ISO 8601 strings is correct as long as
        // both are zulu-time (`...Z`). The issuer always writes zulu.
        if expires_at.as_str() < now_iso {
            return LicenseStatus::Expired {
                email: payload.email,
                expired_at: expires_at.clone(),
            };
        }
    }

    LicenseStatus::Valid {
        email: payload.email,
        tier: payload.tier,
        expires_at: payload.expires_at,
    }
}

/// Tiny helper so callers don't need a chrono dep.
pub fn now_iso_z() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Days since epoch = secs / 86400.
    let days = secs / 86_400;
    // Use chrono-free naive Y/M/D arithmetic. Good enough since we only
    // compare lexicographically with another zulu-time ISO string.
    let (year, month, day) = days_to_ymd(days as i32);
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: i32) -> (i32, u32, u32) {
    days += 719_468; // 1970-01-01 → days since 0000-03-01
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i32) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
