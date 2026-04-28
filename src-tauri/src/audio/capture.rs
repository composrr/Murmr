//! Mic capture via cpal.
//!
//! Two ways to use it:
//!
//! - `record_for_seconds(secs)` — Phase 2 helper that captures a fixed
//!   duration. Still used by the diagnostics button.
//! - `Recorder` — Phase 3 streaming primitive. Call `start()` to open the mic,
//!   `stop()` to drain the buffer, `cancel()` to throw it away. Drives the
//!   real dictation hotkey loop.
//!
//! Phase 4 added an optional RMS callback so the HUD can render a live
//! waveform. The callback fires on every cpal block (typically every 5–20 ms);
//! callers should throttle / debounce on their side if needed.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use parking_lot::Mutex;

#[derive(Debug, Clone)]
pub struct Capture {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub device_name: String,
}

pub type RmsCallback = Arc<dyn Fn(f32) + Send + Sync + 'static>;
pub type ErrorCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;

fn block_rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = data.iter().map(|&s| s * s).sum();
    (sum_sq / data.len() as f32).sqrt()
}

// ---------------------------------------------------------------------------
// Phase-2 helper: fixed-duration capture.
// ---------------------------------------------------------------------------

pub fn record_for_seconds(secs: u32) -> Result<Capture, String> {
    record_for_seconds_with_rms(secs, None)
}

/// Like `record_for_seconds` but lets the caller observe per-block RMS so a
/// UI can render a live waveform during the test capture.
pub fn record_for_seconds_with_rms(
    secs: u32,
    on_rms: Option<RmsCallback>,
) -> Result<Capture, String> {
    let recorder = Recorder::new();
    recorder.start_with_rms(on_rms)?;
    thread::sleep(Duration::from_secs(secs as u64));
    recorder
        .stop()?
        .ok_or_else(|| "recording was cancelled".to_string())
}

// ---------------------------------------------------------------------------
// Phase-3 primitive: start/stop streaming capture.
// ---------------------------------------------------------------------------

struct Active {
    cmd_tx: crossbeam_channel::Sender<StreamCmd>,
    result_rx: crossbeam_channel::Receiver<Capture>,
}

enum StreamCmd {
    Stop,
    Cancel,
}

#[derive(Default)]
pub struct Recorder {
    active: Mutex<Option<Active>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin capturing. If `on_rms` is supplied it fires once per cpal block
    /// with the block's RMS (square-root of mean-square amplitude in [0, ~1]).
    pub fn start_with_rms(&self, on_rms: Option<RmsCallback>) -> Result<(), String> {
        self.start_with_callbacks(on_rms, None)
    }

    pub fn start_with_callbacks(
        &self,
        on_rms: Option<RmsCallback>,
        on_error: Option<ErrorCallback>,
    ) -> Result<(), String> {
        let mut slot = self.active.lock();
        if slot.is_some() {
            return Err("already recording".into());
        }

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<StreamCmd>();
        let (result_tx, result_rx) = crossbeam_channel::bounded::<Capture>(1);
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);

        // The cpal Stream type is !Send on Windows, so we have to park it on
        // its own thread and drive it via channels.
        thread::spawn(move || {
            let init = (|| -> Result<(cpal::Stream, Arc<Mutex<Vec<f32>>>, Capture), String> {
                let host = cpal::default_host();
                let device = host
                    .default_input_device()
                    .ok_or_else(|| "no default input device".to_string())?;

                let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());
                let supported = device
                    .default_input_config()
                    .map_err(|e| format!("failed to query default input config: {e}"))?;

                let sample_rate = supported.sample_rate().0;
                let channels = supported.channels();
                let sample_format = supported.sample_format();
                let stream_config: cpal::StreamConfig = supported.into();

                let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
                    sample_rate as usize * channels as usize * 4,
                )));
                let err_cb = on_error.clone();
                let err_fn = move |err: cpal::StreamError| {
                    let msg = err.to_string();
                    eprintln!("[capture] cpal stream error: {msg}");
                    if let Some(cb) = &err_cb {
                        cb(msg);
                    }
                };

                let rms_cb = on_rms.clone();

                let stream = match sample_format {
                    SampleFormat::F32 => {
                        let buf = Arc::clone(&buffer);
                        let cb = rms_cb.clone();
                        device.build_input_stream(
                            &stream_config,
                            move |data: &[f32], _: &_| {
                                buf.lock().extend_from_slice(data);
                                if let Some(cb) = &cb {
                                    cb(block_rms(data));
                                }
                            },
                            err_fn,
                            None,
                        )
                    }
                    SampleFormat::I16 => {
                        let buf = Arc::clone(&buffer);
                        let cb = rms_cb.clone();
                        device.build_input_stream(
                            &stream_config,
                            move |data: &[i16], _: &_| {
                                let mut converted: Vec<f32> = data
                                    .iter()
                                    .map(|&s| s as f32 / i16::MAX as f32)
                                    .collect();
                                if let Some(cb) = &cb {
                                    cb(block_rms(&converted));
                                }
                                buf.lock().append(&mut converted);
                            },
                            err_fn,
                            None,
                        )
                    }
                    SampleFormat::U16 => {
                        let buf = Arc::clone(&buffer);
                        let cb = rms_cb.clone();
                        device.build_input_stream(
                            &stream_config,
                            move |data: &[u16], _: &_| {
                                let mut converted: Vec<f32> = data
                                    .iter()
                                    .map(|&s| (s as f32 - 32_768.0) / 32_768.0)
                                    .collect();
                                if let Some(cb) = &cb {
                                    cb(block_rms(&converted));
                                }
                                buf.lock().append(&mut converted);
                            },
                            err_fn,
                            None,
                        )
                    }
                    fmt => return Err(format!("unsupported sample format: {fmt:?}")),
                }
                .map_err(|e| format!("failed to build input stream: {e}"))?;

                stream
                    .play()
                    .map_err(|e| format!("failed to start input stream: {e}"))?;

                let metadata = Capture {
                    samples: Vec::new(),
                    sample_rate,
                    channels,
                    device_name,
                };
                Ok((stream, buffer, metadata))
            })();

            let (stream, buffer, metadata) = match init {
                Ok(parts) => {
                    let _ = ready_tx.send(Ok(()));
                    parts
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };

            // Now wait for stop / cancel.
            let cmd = cmd_rx.recv().unwrap_or(StreamCmd::Cancel);
            drop(stream);
            match cmd {
                StreamCmd::Stop => {
                    let mut cap = metadata;
                    cap.samples = std::mem::take(&mut *buffer.lock());
                    let _ = result_tx.send(cap);
                }
                StreamCmd::Cancel => {}
            }
        });

        ready_rx
            .recv_timeout(Duration::from_millis(2500))
            .map_err(|_| "timed out waiting for capture thread to start".to_string())??;

        *slot = Some(Active { cmd_tx, result_rx });
        Ok(())
    }

    /// Stop recording and return the captured buffer + metadata.
    pub fn stop(&self) -> Result<Option<Capture>, String> {
        let active = match self.active.lock().take() {
            Some(a) => a,
            None => return Err("not recording".into()),
        };
        active
            .cmd_tx
            .send(StreamCmd::Stop)
            .map_err(|e| format!("failed to signal stop: {e}"))?;
        active
            .result_rx
            .recv_timeout(Duration::from_millis(2000))
            .map(Some)
            .map_err(|_| "timed out collecting capture buffer".to_string())
    }

    /// Discard the current buffer without producing output.
    pub fn cancel(&self) -> Result<(), String> {
        let active = match self.active.lock().take() {
            Some(a) => a,
            None => return Ok(()),
        };
        active
            .cmd_tx
            .send(StreamCmd::Cancel)
            .map_err(|e| format!("failed to signal cancel: {e}"))?;
        Ok(())
    }
}
