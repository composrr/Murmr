//! GPU backend selection and crash-safety.
//!
//! Murmr is a background app that must never crash or lose a dictation, and
//! GPU drivers are the least trustworthy code we load. Three layers keep a
//! bad driver from hurting the user:
//!
//! 1. **Probe in a child process (Windows).** The first launch of each app
//!    version runs `murmr.exe --murmr-gpu-probe <model>` which loads the base
//!    model on the GPU and decodes a second of audio. If that crashes, hangs
//!    or finds no device, only the child dies; the result is cached in
//!    `<app_data>/gpu-probe.json` and the app stays on CPU.
//! 2. **In-flight marker.** Every GPU model load / decode writes
//!    `<app_data>/gpu-inflight.marker` and removes it afterwards. If the
//!    process dies mid-GPU-work, the marker survives and the next launch
//!    marks the GPU bad and runs on CPU.
//! 3. **Driver isolation when on CPU (Windows).** When the GPU is off (force
//!    CPU, or a failed probe), `VK_LOADER_DRIVERS_DISABLE=*` is set before
//!    ggml initializes, so no Vulkan driver code is loaded into the process.
//!    Implicit Vulkan layers (overlays from OBS, Steam, RivaTuner, …) are
//!    always disabled: Murmr never renders, and those layers are a common
//!    source of crashes in non-game processes.
//!
//! macOS (Apple Silicon) uses Metal, which ships with the OS; layers 2 and the
//! `force_cpu` setting still apply, the child-process probe is skipped.

use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use whisper_rs::whisper_rs_sys::{
    ggml_backend_dev_count, ggml_backend_dev_description, ggml_backend_dev_get,
    ggml_backend_dev_type, ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU,
    ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU,
};

use crate::perf_log;

pub const PROBE_ARG: &str = "--murmr-gpu-probe";
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const CACHE_FILE: &str = "gpu-probe.json";
const MARKER_FILE: &str = "gpu-inflight.marker";

static APP_DATA: OnceLock<PathBuf> = OnceLock::new();

/// Whether this build was compiled with a GPU backend at all.
pub const fn compiled_with_gpu() -> bool {
    cfg!(any(windows, all(target_os = "macos", target_arch = "aarch64")))
}

/// Name of the compiled GPU backend.
pub const fn backend_kind() -> &'static str {
    if cfg!(target_os = "macos") {
        "Metal"
    } else {
        "Vulkan"
    }
}

/// The GPU whisper.cpp will pick when `use_gpu` is set — the first device of
/// type GPU/IGPU in ggml's registry (mirrors `whisper_backend_init_gpu`).
/// Calling this initializes ggml's backend registry.
pub fn first_gpu_device() -> Option<String> {
    if !compiled_with_gpu() {
        return None;
    }
    unsafe {
        let n = ggml_backend_dev_count();
        for i in 0..n {
            let dev = ggml_backend_dev_get(i);
            if dev.is_null() {
                continue;
            }
            let t = ggml_backend_dev_type(dev);
            if t == ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU
                || t == ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU
            {
                let desc = ggml_backend_dev_description(dev);
                let name = if desc.is_null() {
                    "GPU".to_string()
                } else {
                    CStr::from_ptr(desc).to_string_lossy().trim().to_string()
                };
                return Some(name);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Persistent decision
// ---------------------------------------------------------------------------

/// Result of deciding whether the GPU may be used in this process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuVerdict {
    /// App version the verdict was made for — a new version re-probes.
    pub version: String,
    /// A usable GPU was found and passed the probe.
    pub ok: bool,
    /// The GPU's name, when one was found.
    pub device: Option<String>,
    /// Why the GPU isn't used (shown in Settings).
    pub note: Option<String>,
}

fn data_path(name: &str) -> Option<PathBuf> {
    APP_DATA.get().map(|d| d.join(name))
}

fn read_cache() -> Option<GpuVerdict> {
    let raw = std::fs::read_to_string(data_path(CACHE_FILE)?).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_cache(v: &GpuVerdict) {
    if let Some(p) = data_path(CACHE_FILE) {
        if let Ok(s) = serde_json::to_string_pretty(v) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// Forget the cached verdict so the next launch probes again ("Retry GPU").
pub fn clear_cache() {
    if let Some(p) = data_path(CACHE_FILE) {
        let _ = std::fs::remove_file(p);
    }
    if let Some(p) = data_path(MARKER_FILE) {
        let _ = std::fs::remove_file(p);
    }
}

fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Startup decision, made BEFORE any model loads (so the Vulkan-loader env
/// vars below are in place when ggml first initializes).
///
/// Returns `Some(verdict)` when the answer is known now. Returns `None` when
/// the Windows probe still has to run — the caller starts on CPU and calls
/// [`run_probe_blocking`] in the background, hot-swapping to GPU on success.
pub fn startup_verdict(app_data_dir: &Path, force_cpu: bool) -> Option<GpuVerdict> {
    let _ = APP_DATA.set(app_data_dir.to_path_buf());
    isolate_vulkan_layers();

    if !compiled_with_gpu() {
        return Some(GpuVerdict {
            version: app_version(),
            ok: false,
            device: None,
            note: Some("This build has no GPU backend (Intel Mac).".into()),
        });
    }

    // A GPU load/decode was in flight when the previous session died.
    if let Some(marker) = data_path(MARKER_FILE) {
        if marker.exists() {
            let _ = std::fs::remove_file(&marker);
            let v = GpuVerdict {
                version: app_version(),
                ok: false,
                device: read_cache().and_then(|c| c.device),
                note: Some(
                    "Murmr switched to CPU after a GPU problem in a previous session.".into(),
                ),
            };
            perf_log::append("[gpu] in-flight marker found → previous session died in GPU code; using CPU");
            write_cache(&v);
            disable_gpu_drivers_in_process();
            return Some(v);
        }
    }

    let cached = read_cache().filter(|c| c.version == app_version());

    if force_cpu {
        // Keep driver code out of the process entirely. Toggling the GPU back
        // on then needs a restart (the Settings UI says so).
        disable_gpu_drivers_in_process();
        return Some(GpuVerdict {
            version: app_version(),
            ok: false,
            device: cached.as_ref().and_then(|c| c.device.clone()),
            note: Some("GPU turned off in Settings (Use CPU only).".into()),
        });
    }

    if let Some(c) = cached {
        if !c.ok {
            disable_gpu_drivers_in_process();
        }
        return Some(c);
    }

    if cfg!(target_os = "macos") {
        // Metal on Apple Silicon: part of the OS, no driver lottery.
        return Some(GpuVerdict { version: app_version(), ok: true, device: None, note: None });
    }

    None // Windows, first launch of this version → probe in background.
}

/// Run the child-process probe (Windows). Blocks up to [`PROBE_TIMEOUT`];
/// call from a background thread. Caches and returns the verdict.
pub fn run_probe_blocking(probe_model: &Path) -> GpuVerdict {
    let t0 = Instant::now();
    let verdict = match spawn_probe(probe_model) {
        Ok(device) => GpuVerdict { version: app_version(), ok: true, device: Some(device), note: None },
        Err(note) => GpuVerdict { version: app_version(), ok: false, device: None, note: Some(note) },
    };
    perf_log::append(&format!(
        "[gpu] probe finished in {}ms: ok={} device={:?} note={:?}",
        t0.elapsed().as_millis(),
        verdict.ok,
        verdict.device,
        verdict.note
    ));
    write_cache(&verdict);
    verdict
}

fn spawn_probe(probe_model: &Path) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().map_err(|e| format!("probe: current_exe: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.arg(PROBE_ARG)
        .arg(probe_model)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("GPU check could not start: {e}"))?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("GPU check timed out; using CPU.".into());
            }
            Err(e) => return Err(format!("GPU check failed: {e}")),
        }
    };
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    let line = out.lines().find(|l| l.starts_with("MURMR_GPU_PROBE ")).unwrap_or("");
    let rest = line.trim_start_matches("MURMR_GPU_PROBE ").trim();
    match (status.code(), rest.strip_prefix("ok ")) {
        (Some(0), Some(device)) => Ok(device.trim().to_string()),
        (Some(2), _) => Err("No compatible GPU found; using CPU.".into()),
        (code, _) => Err(format!(
            "GPU check failed (exit {}{}); using CPU.",
            code.map(|c| c.to_string()).unwrap_or_else(|| "signal".into()),
            if rest.is_empty() { String::new() } else { format!(": {rest}") }
        )),
    }
}

/// Child-process entry point. Call first thing in `run()`: when argv holds
/// [`PROBE_ARG`], probe the GPU and exit with a status code — never returns.
pub fn maybe_run_probe_and_exit() {
    let args: Vec<String> = std::env::args().collect();
    let Some(i) = args.iter().position(|a| a == PROBE_ARG) else {
        return;
    };
    isolate_vulkan_layers();
    let model = args.get(i + 1).cloned().unwrap_or_default();
    let code = probe_in_this_process(&model);
    std::process::exit(code);
}

fn probe_in_this_process(model: &str) -> i32 {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let Some(device) = first_gpu_device() else {
        println!("MURMR_GPU_PROBE none");
        return 2;
    };
    let mut cp = WhisperContextParameters::default();
    cp.use_gpu(true);
    cp.flash_attn(true);
    let ctx = match WhisperContext::new_with_params(model, cp) {
        Ok(c) => c,
        Err(e) => {
            println!("MURMR_GPU_PROBE load-failed {e}");
            return 3;
        }
    };
    let mut state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => {
            println!("MURMR_GPU_PROBE state-failed {e}");
            return 4;
        }
    };
    // 1.5 s of a soft tone + noise: enough to run encoder + a few decoder
    // steps (compiles the GPU pipelines the real dictation will use).
    let samples: Vec<f32> = (0..24_000)
        .map(|i| {
            let t = i as f32 / 16_000.0;
            0.05 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                + 0.01 * (((i * 7919) % 1000) as f32 / 1000.0 - 0.5)
        })
        .collect();
    let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    p.set_n_threads(4);
    p.set_language(Some("en"));
    p.set_print_progress(false);
    p.set_print_realtime(false);
    p.set_print_special(false);
    p.set_print_timestamps(false);
    p.set_no_context(true);
    if let Err(e) = state.full(p, &samples) {
        println!("MURMR_GPU_PROBE decode-failed {e}");
        return 5;
    }
    println!("MURMR_GPU_PROBE ok {device}");
    0
}

// ---------------------------------------------------------------------------
// In-flight marker
// ---------------------------------------------------------------------------

/// RAII marker around GPU work; see module docs. Nested/concurrent guards are
/// fine — the marker is just a file that exists while any GPU work runs.
pub struct InflightGuard(());

static INFLIGHT: parking_lot::Mutex<usize> = parking_lot::const_mutex(0);

impl InflightGuard {
    pub fn begin() -> Self {
        let mut n = INFLIGHT.lock();
        if *n == 0 {
            if let Some(p) = data_path(MARKER_FILE) {
                let _ = std::fs::write(p, b"gpu work in flight");
            }
        }
        *n += 1;
        InflightGuard(())
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        let mut n = INFLIGHT.lock();
        *n = n.saturating_sub(1);
        if *n == 0 {
            if let Some(p) = data_path(MARKER_FILE) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Vulkan loader environment (Windows)
// ---------------------------------------------------------------------------

fn set_env_if_unset(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

/// Disable implicit Vulkan layers (overlays) for this process. The Khronos
/// loader reads these via GetEnvironmentVariable, so setting them at runtime
/// works as long as it happens before ggml creates its Vulkan instance.
fn isolate_vulkan_layers() {
    if cfg!(windows) {
        set_env_if_unset("VK_LOADER_LAYERS_DISABLE", "~implicit~");
    }
}

/// Hide every Vulkan driver from this process: ggml then finds zero Vulkan
/// devices and never runs driver code. Must run before the first model load.
fn disable_gpu_drivers_in_process() {
    if cfg!(windows) {
        set_env_if_unset("VK_LOADER_DRIVERS_DISABLE", "*");
        DRIVERS_DISABLED.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

static DRIVERS_DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True when loading a model can run GPU-driver code in this process.
pub fn driver_code_may_run() -> bool {
    compiled_with_gpu() && !drivers_disabled()
}

/// Clean shutdown: GPU work interrupted by a normal quit is not a crash.
pub fn clear_inflight_marker() {
    if let Some(p) = data_path(MARKER_FILE) {
        let _ = std::fs::remove_file(p);
    }
}

/// GPU drivers were kept out of this process at startup; using the GPU now
/// needs a restart.
pub fn drivers_disabled() -> bool {
    DRIVERS_DISABLED.load(std::sync::atomic::Ordering::SeqCst)
}
