//! The live speech engine: owns the loaded [`Transcriber`] and swaps it for a
//! new one (different model, GPU on/off, VAD on/off) without restarting.
//!
//! Swap safety: callers take an `Arc<Transcriber>` snapshot via
//! [`Engine::current`] for the duration of one dictation. A swap only replaces
//! the pointer; an in-flight transcription keeps its own `Arc` and finishes on
//! the old model, which is freed when the last snapshot drops. Loads run on a
//! background thread, one at a time, and a failed load keeps the old model.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::gpu::{self, GpuVerdict};
use super::models::{self, BASE_MODEL, VAD_MODEL};
use super::{BackendInfo, Transcriber};
use crate::perf_log;
use crate::settings::SettingsStore;

/// Snapshot for Settings → Advanced. Emitted as `murmr:engine-status`.
#[derive(Serialize, Clone, Debug)]
pub struct EngineStatus {
    /// Model file actually loaded (may differ from the setting after a fallback).
    pub model_name: String,
    /// Backend actually in use.
    pub backend: BackendInfo,
    /// A usable GPU exists on this machine (passed the check).
    pub gpu_available: bool,
    /// The GPU check is still running (first launch of a version, Windows).
    pub gpu_pending: bool,
    pub gpu_device: Option<String>,
    /// Why the GPU isn't in use, when it isn't.
    pub gpu_note: Option<String>,
    /// Turning the GPU back on needs an app restart (driver code was kept out
    /// of this process).
    pub restart_required: bool,
    /// Silero VAD is active for the loaded model.
    pub vad_active: bool,
    /// A model load is in progress (name of the model being loaded).
    pub loading: Option<String>,
    /// Last load error, if the most recent load failed.
    pub last_error: Option<String>,
}

/// What the settings ask for.
#[derive(Clone, PartialEq, Debug)]
struct Wanted {
    model: String,
    gpu: bool,
    vad: bool,
}

pub struct Engine {
    current: RwLock<Arc<Transcriber>>,
    loaded: Mutex<Wanted>,
    status: Mutex<EngineStatus>,
    verdict: Mutex<Option<GpuVerdict>>,
    load_lock: Mutex<()>,
    settings: Arc<SettingsStore>,
    resolve: fn(&str) -> PathBuf,
    app: Mutex<Option<AppHandle>>,
    retry_requested: AtomicBool,
}

impl Engine {
    /// Load the startup model synchronously (falls back to the bundled base
    /// model). On Windows' first launch of a version, the GPU check runs in the
    /// background afterwards and hot-swaps to the GPU when it passes.
    pub fn start(
        app_data_dir: &Path,
        settings: Arc<SettingsStore>,
        resolve: fn(&str) -> PathBuf,
    ) -> Result<Arc<Self>, String> {
        models::init(app_data_dir);
        models::clean_partial_downloads();
        let s = settings.get();
        let verdict = gpu::startup_verdict(app_data_dir, s.force_cpu);
        perf_log::append(&format!("[engine] startup GPU verdict: {verdict:?}"));
        let wanted = Self::wanted_for(&s, verdict.as_ref());
        let t0 = Instant::now();
        let (transcriber, loaded) = Self::load_with_fallback(&wanted, resolve)?;
        perf_log::append(&format!(
            "[engine] loaded model={} backend={} vad={} in {}ms",
            transcriber.model_name(),
            transcriber.backend().label(),
            transcriber.has_vad(),
            t0.elapsed().as_millis()
        ));

        let engine = Arc::new(Self {
            status: Mutex::new(EngineStatus {
                model_name: transcriber.model_name().to_string(),
                backend: transcriber.backend().clone(),
                gpu_available: false,
                gpu_pending: false,
                gpu_device: None,
                gpu_note: None,
                restart_required: false,
                vad_active: transcriber.has_vad(),
                loading: None,
                last_error: None,
            }),
            current: RwLock::new(Arc::new(transcriber)),
            loaded: Mutex::new(loaded),
            verdict: Mutex::new(verdict.clone()),
            load_lock: Mutex::new(()),
            settings,
            resolve,
            app: Mutex::new(None),
            retry_requested: AtomicBool::new(false),
        });
        engine.refresh_status_fields();

        if verdict.is_none() {
            let e = Arc::clone(&engine);
            std::thread::Builder::new()
                .name("murmr-gpu-probe".into())
                .spawn(move || {
                    let v = gpu::run_probe_blocking(&resolve(BASE_MODEL));
                    *e.verdict.lock() = Some(v);
                    e.refresh_status_fields();
                    e.emit_status();
                    e.reload_blocking();
                })
                .map_err(|e| format!("spawn probe thread: {e}"))?;
        } else if engine.current().backend().is_gpu() {
            engine.spawn_warmup();
        }
        Ok(engine)
    }

    /// Give the engine an AppHandle so it can emit status events.
    pub fn attach_app(&self, app: AppHandle) {
        *self.app.lock() = Some(app);
    }

    /// The model to use for one dictation. Hold it until the dictation ends.
    pub fn current(&self) -> Arc<Transcriber> {
        Arc::clone(&self.current.read())
    }

    pub fn status(&self) -> EngineStatus {
        self.status.lock().clone()
    }

    /// Re-read settings and swap the model if model / GPU / VAD changed.
    /// Returns immediately; the load happens on a background thread.
    pub fn request_reload(self: &Arc<Self>) {
        let e = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("murmr-model-load".into())
            .spawn(move || e.reload_blocking());
    }

    /// Forget the GPU verdict; the next launch checks the GPU again.
    pub fn retry_gpu_detection(&self) {
        gpu::clear_cache();
        self.retry_requested.store(true, Ordering::SeqCst);
        self.refresh_status_fields();
        self.emit_status();
    }

    // -----------------------------------------------------------------------

    fn wanted_for(s: &crate::settings::Settings, verdict: Option<&GpuVerdict>) -> Wanted {
        let gpu_ok = verdict.map(|v| v.ok).unwrap_or(false);
        Wanted {
            model: s.model_name.clone(),
            gpu: gpu_ok && !s.force_cpu && !gpu::drivers_disabled(),
            vad: s.vad_enabled,
        }
    }

    fn vad_path(resolve: fn(&str) -> PathBuf) -> Option<PathBuf> {
        let p = resolve(VAD_MODEL);
        p.is_file().then_some(p)
    }

    fn load(wanted: &Wanted, resolve: fn(&str) -> PathBuf) -> Result<Transcriber, String> {
        let path = resolve(&wanted.model);
        let vad = if wanted.vad { Self::vad_path(resolve) } else { None };
        Transcriber::new(&path.to_string_lossy(), wanted.gpu, vad.as_deref())
    }

    /// Startup load: the selected model, then the bundled base model (on the
    /// GPU if wanted), then base on CPU — the pre-GPU behaviour.
    fn load_with_fallback(
        wanted: &Wanted,
        resolve: fn(&str) -> PathBuf,
    ) -> Result<(Transcriber, Wanted), String> {
        let mut chain = vec![wanted.clone()];
        for fb in [
            Wanted { model: BASE_MODEL.into(), gpu: wanted.gpu, vad: wanted.vad },
            Wanted { model: BASE_MODEL.into(), gpu: false, vad: wanted.vad },
        ] {
            if !chain.contains(&fb) {
                chain.push(fb);
            }
        }
        let mut last_err = String::new();
        for w in chain {
            match Self::load(&w, resolve) {
                Ok(t) => return Ok((t, w)),
                Err(e) => {
                    perf_log::append(&format!(
                        "[engine] '{}' (gpu={}) failed to load: {e} — trying next fallback",
                        w.model, w.gpu
                    ));
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    fn reload_blocking(self: &Arc<Self>) {
        let _one_at_a_time = self.load_lock.lock();
        let s = self.settings.get();
        let wanted = Self::wanted_for(&s, self.verdict.lock().as_ref());
        if *self.loaded.lock() == wanted {
            self.refresh_status_fields();
            self.emit_status();
            return;
        }
        {
            let mut st = self.status.lock();
            st.loading = Some(wanted.model.clone());
            st.last_error = None;
        }
        self.emit_status();
        let t0 = Instant::now();
        perf_log::append(&format!("[engine] hot-swap → {wanted:?}"));
        match Self::load(&wanted, self.resolve) {
            Ok(t) => {
                let t = Arc::new(t);
                if t.backend().is_gpu() {
                    warm_up(&t);
                }
                perf_log::append(&format!(
                    "[engine] hot-swap done in {}ms: model={} backend={} vad={}",
                    t0.elapsed().as_millis(),
                    t.model_name(),
                    t.backend().label(),
                    t.has_vad()
                ));
                *self.current.write() = t;
                *self.loaded.lock() = wanted;
                self.status.lock().loading = None;
            }
            Err(e) => {
                perf_log::append(&format!("[engine] hot-swap failed, keeping current model: {e}"));
                let mut st = self.status.lock();
                st.loading = None;
                st.last_error = Some(e);
            }
        }
        self.refresh_status_fields();
        self.emit_status();
    }

    fn spawn_warmup(self: &Arc<Self>) {
        let t = self.current();
        let _ = std::thread::Builder::new()
            .name("murmr-gpu-warmup".into())
            .spawn(move || warm_up(&t));
    }

    fn refresh_status_fields(&self) {
        let cur = self.current();
        let verdict = self.verdict.lock().clone();
        let s = self.settings.get();
        let mut st = self.status.lock();
        st.model_name = cur.model_name().to_string();
        st.backend = cur.backend().clone();
        st.vad_active = cur.has_vad();
        st.gpu_pending = verdict.is_none() && gpu::compiled_with_gpu();
        let gpu_ok = verdict.as_ref().map(|v| v.ok).unwrap_or(false);
        st.gpu_available = gpu_ok;
        st.gpu_device = cur
            .backend()
            .device
            .clone()
            .or_else(|| verdict.as_ref().and_then(|v| v.device.clone()));
        let retry = self.retry_requested.load(Ordering::SeqCst);
        let restart = gpu_ok && !s.force_cpu && gpu::drivers_disabled();
        st.restart_required = restart || retry;
        st.gpu_note = if retry {
            Some("Restart Murmr to check the GPU again.".into())
        } else if cur.backend().is_gpu() {
            None
        } else if restart {
            Some("Restart Murmr to switch to the GPU.".into())
        } else if s.force_cpu {
            Some("GPU turned off (Use CPU only).".into())
        } else if st.gpu_pending {
            Some("Checking the GPU…".into())
        } else {
            verdict.and_then(|v| v.note).or_else(|| {
                gpu_ok.then(|| "GPU found but not in use.".to_string())
            })
        };
    }

    fn emit_status(&self) {
        if let Some(app) = self.app.lock().as_ref() {
            let _ = app.emit("murmr:engine-status", self.status());
        }
    }
}

/// Run a short decode so the GPU compiles its pipelines now, not during the
/// user's first dictation.
fn warm_up(t: &Transcriber) {
    let t0 = Instant::now();
    let samples: Vec<f32> = (0..16_000)
        .map(|i| 0.02 * (2.0 * std::f32::consts::PI * 180.0 * i as f32 / 16_000.0).sin())
        .collect();
    let r = t.transcribe(&samples, None, false);
    perf_log::append(&format!(
        "[engine] GPU warm-up {}ms ok={}",
        t0.elapsed().as_millis(),
        r.is_ok()
    ));
}
