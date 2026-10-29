//! Whisper model files: the bundled fallback, optional downloadable models,
//! and the Silero VAD model.
//!
//! - Bundled (read-only, ships with the installer): `ggml-base.en.bin` and the
//!   VAD model `ggml-silero-v5.1.2.bin`, in the app's resource `models/` dir.
//! - Downloaded (on demand, from the ggerganov/whisper.cpp Hugging Face repo,
//!   pinned to a commit and SHA-256-verified): `<app_data>/models/`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::perf_log;

pub const BASE_MODEL: &str = "ggml-base.en.bin";
pub const VAD_MODEL: &str = "ggml-silero-v5.1.2.bin";

/// A model Murmr knows how to download.
pub struct CatalogModel {
    pub file: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
}

pub const CATALOG: &[CatalogModel] = &[CatalogModel {
    file: "ggml-large-v3-turbo-q5_0.bin",
    label: "Large v3 Turbo",
    description: "Much more accurate than Base, especially on names, jargon and accents. Fast on a GPU; slow on CPU.",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin",
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    size_bytes: 574_041_195,
}];

static DOWNLOAD_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Set once at startup (before any model resolution).
pub fn init(app_data_dir: &Path) {
    let dir = app_data_dir.join("models");
    let _ = std::fs::create_dir_all(&dir);
    let _ = DOWNLOAD_DIR.set(dir);
}

/// `<app_data>/models` — where downloaded models live.
pub fn download_dir() -> Option<&'static Path> {
    DOWNLOAD_DIR.get().map(|p| p.as_path())
}

/// A downloaded model file, if present and complete.
pub fn downloaded_path(file: &str) -> Option<PathBuf> {
    let p = download_dir()?.join(file);
    p.is_file().then_some(p)
}

/// True for files that are Whisper speech models (not the VAD model, not a
/// partial download).
pub fn is_speech_model_file(name: &str) -> bool {
    name.starts_with("ggml-") && name.ends_with(".bin") && !name.starts_with("ggml-silero")
}

#[derive(Serialize, Clone)]
pub struct CatalogEntry {
    pub file: String,
    pub label: String,
    pub description: String,
    pub size_bytes: u64,
    pub downloaded: bool,
}

pub fn catalog_entries() -> Vec<CatalogEntry> {
    CATALOG
        .iter()
        .map(|m| CatalogEntry {
            file: m.file.into(),
            label: m.label.into(),
            description: m.description.into(),
            size_bytes: m.size_bytes,
            downloaded: downloaded_path(m.file).is_some(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

/// Progress event payload, emitted as `murmr:model-download`.
#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub file: String,
    pub downloaded: u64,
    pub total: u64,
    /// "downloading" | "verifying" | "done" | "error" | "cancelled"
    pub state: String,
    pub error: Option<String>,
}

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn cancel_download() {
    CANCEL.store(true, Ordering::SeqCst);
}

struct DownloadSlot;
impl Drop for DownloadSlot {
    fn drop(&mut self) {
        DOWNLOADING.store(false, Ordering::SeqCst);
    }
}

/// Download a catalog model into `<app_data>/models/`, streaming to a
/// `.part` file, verifying size + SHA-256, then renaming into place. The
/// final file only ever appears complete and verified. `on_progress` is
/// called ~10×/s.
pub async fn download(
    file: &str,
    on_progress: impl Fn(DownloadProgress),
) -> Result<PathBuf, String> {
    let model = CATALOG
        .iter()
        .find(|m| m.file == file)
        .ok_or_else(|| format!("unknown model {file}"))?;
    let dir = download_dir().ok_or("models folder not initialised")?.to_path_buf();
    if let Some(p) = downloaded_path(model.file) {
        return Ok(p);
    }
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("another model download is already running".into());
    }
    let _slot = DownloadSlot;
    CANCEL.store(false, Ordering::SeqCst);

    let final_path = dir.join(model.file);
    let part_path = dir.join(format!("{}.part", model.file));
    let progress = |downloaded: u64, state: &str, error: Option<String>| DownloadProgress {
        file: model.file.into(),
        downloaded,
        total: model.size_bytes,
        state: state.into(),
        error,
    };

    let result = async {
        // reqwest is built with `rustls-no-provider`; install the same ring
        // provider tauri-plugin-updater uses (no-op if already installed).
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let client = reqwest::Client::builder()
            .user_agent(concat!("Murmr/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            // Per-read timeout so a stalled connection can't hang forever.
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        let mut resp = client
            .get(model.url)
            .send()
            .await
            .map_err(|e| format!("download failed to start: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("download failed: HTTP {}", resp.status()));
        }
        let mut out = std::fs::File::create(&part_path)
            .map_err(|e| format!("can't write {part_path:?}: {e}"))?;
        let mut hasher = Sha256::new();
        let mut got: u64 = 0;
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        loop {
            if CANCEL.load(Ordering::SeqCst) {
                return Err("cancelled".into());
            }
            let chunk = resp
                .chunk()
                .await
                .map_err(|e| format!("download interrupted: {e}"))?;
            let Some(chunk) = chunk else { break };
            hasher.update(&chunk);
            out.write_all(&chunk).map_err(|e| format!("disk write failed: {e}"))?;
            got += chunk.len() as u64;
            if got > model.size_bytes + 1024 * 1024 {
                return Err("download is larger than expected".into());
            }
            if last_emit.elapsed() >= Duration::from_millis(100) {
                last_emit = Instant::now();
                on_progress(progress(got, "downloading", None));
            }
        }
        out.flush().map_err(|e| format!("disk write failed: {e}"))?;
        drop(out);
        on_progress(progress(got, "verifying", None));
        if got != model.size_bytes {
            return Err(format!(
                "download incomplete ({got} of {} bytes)",
                model.size_bytes
            ));
        }
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        if hex != model.sha256 {
            return Err("checksum mismatch — the file was corrupted in transit".into());
        }
        std::fs::rename(&part_path, &final_path)
            .map_err(|e| format!("couldn't move model into place: {e}"))?;
        Ok(final_path.clone())
    }
    .await;

    match &result {
        Ok(p) => {
            perf_log::append(&format!("[model] downloaded + verified {p:?}"));
            on_progress(progress(model.size_bytes, "done", None));
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part_path);
            perf_log::append(&format!("[model] download of {} failed: {e}", model.file));
            let state = if e == "cancelled" { "cancelled" } else { "error" };
            on_progress(progress(0, state, Some(e.clone())));
        }
    }
    result
}

/// Delete a downloaded model (only files in the download dir).
pub fn delete_downloaded(file: &str) -> Result<(), String> {
    if !CATALOG.iter().any(|m| m.file == file) {
        return Err(format!("{file} is not a downloadable model"));
    }
    if let Some(p) = downloaded_path(file) {
        std::fs::remove_file(&p).map_err(|e| format!("couldn't delete {p:?}: {e}"))?;
    }
    Ok(())
}

/// Remove `.part` leftovers from an interrupted download (app quit mid-way).
pub fn clean_partial_downloads() {
    let Some(dir) = download_dir() else { return };
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().ends_with(".part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}
