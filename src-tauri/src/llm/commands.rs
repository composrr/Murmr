//! Tauri commands for the AI-cleanup settings UI (API key save/test/remove).
//!
//! All of them are async + `spawn_blocking`: keychain access and the
//! blocking HTTP client must not run on the main thread or inside the async
//! runtime directly.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::anthropic::{self, AnthropicProvider};
use super::cleanup::CLEANUP_INSTRUCTION;
use super::{keychain, transform_with_deadline, LlmError, TransformContext};

#[derive(Serialize)]
pub struct KeyStatus {
    has_key: bool,
    /// Masked form for display ("sk-ant-…WXYZ"); never the full key.
    hint: Option<String>,
}

#[derive(Serialize)]
pub struct KeyTestResult {
    ok: bool,
    latency_ms: u64,
    message: String,
}

const TEST_INPUT: &str = "um so this is uh a quick test of murmr's ai cleanup does it work";

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("worker failed: {e}"))
}

#[tauri::command]
pub async fn llm_key_status() -> Result<KeyStatus, String> {
    blocking(|| {
        keychain::get_key()
            .map(|k| KeyStatus {
                has_key: k.is_some(),
                hint: k.as_deref().map(keychain::mask),
            })
            .map_err(|e| e.to_string())
    })
    .await?
}

#[tauri::command]
pub async fn llm_save_key(key: String) -> Result<KeyStatus, String> {
    blocking(move || {
        keychain::save_key(&key).map_err(|e| e.to_string())?;
        Ok(KeyStatus {
            has_key: true,
            hint: Some(keychain::mask(key.trim())),
        })
    })
    .await?
}

#[tauri::command]
pub async fn llm_remove_key() -> Result<(), String> {
    blocking(|| keychain::remove_key().map_err(|e| e.to_string())).await?
}

/// Run a real cleanup on a fixed sample. Uses `key` if given (test before
/// saving), else the saved key. Reports latency so users can judge it.
#[tauri::command]
pub async fn llm_test_key(key: Option<String>) -> Result<KeyTestResult, String> {
    blocking(move || {
        let key = match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
            Some(k) => k,
            None => match keychain::get_key() {
                Ok(Some(k)) => k,
                Ok(None) => return fail(LlmError::MissingKey, 0),
                Err(e) => return fail(e, 0),
            },
        };
        let started = Instant::now();
        let res = transform_with_deadline(
            Arc::new(AnthropicProvider::new(key)),
            TEST_INPUT.to_string(),
            CLEANUP_INSTRUCTION.to_string(),
            TransformContext::default(),
            Duration::from_secs(15),
        );
        let ms = started.elapsed().as_millis() as u64;
        crate::perf_log::append(&format!(
            "[llm] key test ok={} ms={ms}{}",
            res.is_ok(),
            res.as_ref().err().map(|e| format!(" reason={}", e.tag())).unwrap_or_default()
        ));
        match res {
            Ok(text) => KeyTestResult {
                ok: true,
                latency_ms: ms,
                message: text,
            },
            Err(e) => fail(e, ms),
        }
    })
    .await
}

fn fail(e: LlmError, ms: u64) -> KeyTestResult {
    let message = match &e {
        LlmError::Http { status: 401, .. } => "Anthropic rejected this API key.".to_string(),
        LlmError::Http { status: 403, .. } => {
            "This key isn't allowed to use claude-haiku-5-5.".to_string()
        }
        LlmError::Http { status: 429, .. } => "Rate limited — try again in a moment.".to_string(),
        LlmError::MissingKey => "No API key saved.".to_string(),
        other => other.to_string(),
    };
    KeyTestResult {
        ok: false,
        latency_ms: ms,
        message,
    }
}

/// Fire-and-forget connection warm-up, called when a recording starts with
/// cleanup enabled.
pub fn prewarm_if_enabled(settings: &crate::settings::Settings) {
    if settings.ai_cleanup && !settings.literal_mode {
        anthropic::prewarm();
    }
}
