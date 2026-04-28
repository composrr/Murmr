# Murmr

> *Speak. It types.*

Murmr is a privacy-first, cross-platform desktop voice-dictation app — a local
alternative to Wispr Flow / WhisperTyping. Press a key, speak, and your words
are transcribed locally and typed into whatever app you're focused on.
**No internet required, no API keys, no subscription.**

- 🔒 **Local-only** — Whisper runs on your CPU. No audio or text leaves your machine.
- 🎙️ **Tap to toggle, hold for push-to-talk** — both work from one bare-modifier key (`Right Ctrl` by default).
- 🌍 **Works in every app** — Notepad, Word, Chrome, VS Code, Slack, terminal, anywhere you can type.
- ✨ **Cleans up as you go** — removes filler words, fixes punctuation, capitalizes sentences.
- 🗣️ **Voice commands** — say "period", "comma", "new line" and Murmr inserts the right character.
- 📚 **Custom dictionary** — teach it proper nouns, replacements, and snippet expansions.
- 🔁 **Searchable history** — every transcription is saved locally; copy, re-paste, or delete any.

---

## Install (end users)

> **Status:** Murmr is in active development. Pre-built installers will land on
> the GitHub Releases page once `v0.1.0` is tagged. Until then, build from
> source — see [Build from source](#build-from-source) below.

### Windows

1. Download the latest `.msi` from
   [Releases](https://github.com/<your-username>/murmr/releases).
2. Double-click. **Windows SmartScreen will warn you** that the publisher is
   unverified — Murmr is unsigned for now. Click **More info → Run anyway**.
3. The installer drops Murmr into `%LOCALAPPDATA%\Murmr\` and starts it on next
   login if you opt in during onboarding.
4. Murmr lives in the system tray. Right-click for `Open · Pause · Quit`.

### macOS

1. Download the latest `.dmg` from Releases.
2. Drag Murmr into `Applications`.
3. **First launch will be blocked by Gatekeeper** because Murmr is unsigned.
   Open System Settings → Privacy & Security → scroll down to the "Murmr was
   blocked" notice → click **Open Anyway**.
4. macOS will ask for **Microphone** permission and **Accessibility** permission
   on first dictation. Grant both — Accessibility is required for Murmr to type
   into other apps on your behalf.

> **Why isn't Murmr code-signed?** Signing certificates cost money and lock the
> project to specific Apple/Microsoft developer programs. We may revisit after
> the project sees adoption — for now the one-click "trust this app" dance is
> documented above.

---

## Using Murmr

### Hotkeys

| Shortcut | Action |
|---|---|
| **Right Ctrl** (tap, < 250 ms) | Toggle recording — tap again to stop |
| **Right Ctrl** (hold, > 250 ms) | Push-to-talk — release stops recording |
| **Shift + Right Ctrl** | Re-paste the most recent transcription |
| **Esc** while recording | Cancel — discard audio, no insert |

### The HUD

While you're dictating, a dark pill floats just under the focused text field
showing a red recording dot, a live waveform, an elapsed timer, and a running
estimate of words spoken. Then it morphs into a quiet "transcribing" indicator
while Whisper runs (typically 1–3 s on `base.en`). Once your text is pasted,
the pill vanishes.

### Settings

Open the main window (left-click the tray icon, or `Open Murmr` from the menu).
The sidebar has:

- **Home** — chronological transcription history with full-text search and
  copy / re-insert / delete actions per row.
- **Insights** — words-per-minute, total words, time saved, GitHub-style
  streak heatmap, most-used words, filler-words ranking, and milestone-gated
  speaking-habits cards.
- **Dictionary** — unified words / replacements / snippets list with
  inline add/edit/delete.
- **General · Microphone · Hotkeys · Preferences · Advanced** — all the
  configuration you'd expect.

---

## Build from source

### Prerequisites

| Tool | Why | Version |
|---|---|---|
| **Rust** (stable) | Backend | `rustc 1.75+` |
| **Node.js** | Frontend | `v20+` |
| **CMake** | whisper.cpp build | `3.20+` |
| **LLVM (clang)** | bindgen needs `libclang.dll` | `15+` |
| **Visual Studio Build Tools** (Windows) | MSVC + Windows SDK | `2022` (or 2019) |
| **Xcode Command Line Tools** (macOS) | Apple toolchain | latest |

#### One-time install on Windows

```powershell
# Visual Studio Build Tools 2022 (with "Desktop development with C++")
winget install --id Microsoft.VisualStudio.2022.BuildTools --silent

# CMake — bundled with VS or install standalone
winget install --id Kitware.CMake --silent

# LLVM — required for bindgen (whisper-rs builds via bindgen)
winget install --id LLVM.LLVM --silent
```

The repo includes [`scripts/run-tauri.mjs`](scripts/run-tauri.mjs) which
auto-discovers the VS-bundled CMake + MSVC and the standard LLVM install path
and prepends them to `PATH` / sets `LIBCLANG_PATH` so `npm run tauri dev`
works without Developer Command Prompts.

#### One-time install on macOS

```bash
xcode-select --install
brew install cmake llvm
```

### Get the Whisper model

Murmr ships with `ggml-base.en.bin` bundled into the installer, but for local
development you need to download it once into `src-tauri/models/`:

```bash
curl -L -o src-tauri/models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
```

The file is ~150 MB and is gitignored.

### Run the dev build

```bash
npm install
npm run tauri dev
```

First run takes ~5 minutes — Cargo compiles whisper.cpp + a few hundred Rust
crates. Subsequent runs are seconds.

### Project structure

```
murmr/
├── src/                       # React frontend (TypeScript)
│   ├── windows/
│   │   ├── main/              # Sidebar + page router (Home, Insights, …)
│   │   ├── hud/               # Floating dictation pill
│   │   └── onboarding/        # First-run wizard
│   ├── components/            # Shared primitives
│   ├── hooks/                 # useTheme, useDictationStatus
│   └── lib/                   # ipc.ts, theme.ts, format.ts
├── src-tauri/                 # Rust backend
│   ├── src/
│   │   ├── audio/             # cpal capture + rubato resample
│   │   ├── transcribe/        # whisper-rs + post-processing pipeline
│   │   ├── hotkey/            # rdev global keyboard hook
│   │   ├── injector/          # arboard + enigo clipboard-paste
│   │   ├── focus/             # Win32 UIA + caret lookup for HUD positioning
│   │   ├── db/                # SQLite via rusqlite
│   │   ├── settings.rs        # JSON settings store
│   │   ├── sounds.rs          # Synthesized + custom-WAV playback
│   │   ├── controller.rs      # State machine: hotkey → record → transcribe → inject
│   │   └── lib.rs             # Tauri entry, tray, IPC handler registry
│   └── models/ggml-base.en.bin
├── public/fonts/              # Inter + Source Serif 4 (bundled, no Google Fonts)
└── scripts/run-tauri.mjs      # Auto-discovers CMake / MSVC / LLVM on Windows
```

---

## Troubleshooting

### Microphone error: "Microphone error: Stream error"

Your mic was disconnected mid-dictation, or another app exclusively claimed
the device. Check Settings → Privacy → Microphone, and try a different input
in **Settings → Microphone**.

### Right Ctrl doesn't trigger anything

- Make sure Murmr is running — tray icon visible.
- Murmr listens for *Right Ctrl* specifically, not Left Ctrl. They report as
  different keys.
- If you've remapped Right Ctrl via PowerToys / AutoHotkey, that takes
  precedence over our hook.

### "Location is not available" when opening the app data folder

Old Tauri-Windows quirk: the app's identifier ends in `.desktop` and the shell
tries to interpret that as a file extension. Murmr's "Open" buttons explicitly
use `explorer /e,<path>` which avoids the issue. If you hit this after
upgrading from an older build, restart the app.

### Transcription is slow

`base.en` is the default model and balances accuracy/speed on a modern CPU
(~2× real-time). For an even quicker model, swap to `tiny.en` (~75 MB) — Settings
→ Advanced → Speech model file.

### Whisper produces "Thanks for watching!" or other random phrases

Whisper hallucinates common training-data phrases on near-silence. Murmr's
energy-based VAD already drops most of these before transcription, and any
that slip through are filtered against a known hallucination list. If you
still see them, try increasing your mic gain.

---

## Roadmap

- [x] Local Whisper transcription
- [x] Global hotkey + clipboard-paste injection
- [x] Floating HUD with live waveform
- [x] SQLite-backed transcription history + search
- [x] Insights page (stats + heatmap + top words + speaking habits)
- [x] Dictionary (words / replacements / snippets) with CRUD
- [x] First-run onboarding wizard
- [x] Post-processing pipeline (fillers, voice commands, auto-cap)
- [x] Sound effects + custom WAV support
- [x] Settings persistence + launch-at-login + retention auto-purge
- [ ] **Auto-update** — in-app background check + one-click update via Tauri Updater
- [ ] Code-signed installers (Windows + macOS)
- [ ] AI rewrite stage (local LLM for self-correction patterns + tone polish)
- [ ] Streaming partial transcription rendered live in the HUD
- [ ] Per-app injection profiles
- [ ] Multilingual model support
- [ ] Mobile companion app

---

## License

[MIT](LICENSE)
