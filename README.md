# Murmr

> *Speak. It types.*

Murmr is a privacy-first desktop voice-dictation app for Windows and macOS — a
local alternative to Wispr Flow / WhisperTyping. Press a key, speak, and your
words are transcribed on your own computer and typed into whatever app you're
in. **No account, no subscription, and your audio never leaves your machine.**

- 🔒 **Local speech recognition** — Whisper runs on your computer, on the GPU
  when you have one (Metal on Apple Silicon, Vulkan on Windows). Audio is
  never uploaded.
- 🎙️ **Tap to toggle, hold for push-to-talk** — both from one key.
- 🌍 **Works in every app** — Word, Chrome, VS Code, Slack, Terminal, anywhere
  you can type.
- ✨ **Cleans up as you go** — removes filler words, fixes punctuation,
  capitalizes sentences, and skips silence.
- 🗣️ **Voice commands** — say "period", "comma", "new line" and Murmr inserts
  the right character.
- 📚 **Dictionary that learns** — teach it names, replacements and snippets.
  When you correct a misheard word, Murmr offers to add it.
- 🔁 **Searchable history** — every transcription is saved locally; copy,
  re-paste, edit or delete any of them.
- 🤖 **Optional AI features** — with your own Anthropic API key, Murmr can
  polish each dictation (AI cleanup) or rewrite selected text on command
  (Command mode). Off unless you add a key; only text is sent, never audio.

---

## Install

Download the latest installer from
[Releases](https://github.com/composrr/Murmr/releases/latest).

### macOS

1. Download `Murmr_<version>_aarch64.dmg` (Apple Silicon) or
   `Murmr_<version>_x64.dmg` (Intel).
2. Open it and drag Murmr into **Applications**. The app is signed and
   notarized by Apple, so it opens with no Gatekeeper warning.
3. Grant the three permissions Murmr asks for: **Microphone**, **Input
   Monitoring** (to hear the hotkey) and **Accessibility** (to type into other
   apps). Settings → Permissions shows which are on and opens the right System
   Settings page for anything missing.
4. **Pick a hotkey your keyboard has.** Mac keyboards usually have no Right
   Ctrl, so set the dictation key to **Right Option** (shown as `AltGr`) in
   Settings → Hotkeys.

### Windows

1. Download `Murmr_<version>_x64-setup.exe` and run it.
2. **Windows SmartScreen will warn you** that the publisher is unverified —
   the Windows build is not code-signed yet. Click **More info → Run anyway**.
3. Murmr lives in the system tray. Right-click it for **Open Murmr · Pause
   dictation · Quit Murmr**.

### Updates

Murmr checks for new versions itself and shows a banner when one is ready.
You can also check by hand in **Settings → General → Check for updates**.
What changed in each version is in [CHANGELOG.md](CHANGELOG.md) and in the
app's "What's new" view.

---

## Using Murmr

### Hotkeys

| Shortcut | Action |
|---|---|
| **Dictation key** (tap, under 250 ms) | Start recording; tap again to stop |
| **Dictation key** (hold) | Push-to-talk; letting go stops recording |
| **Esc** while recording | Cancel; nothing is typed |
| **Re-paste** (off by default) | Type the most recent transcription again |
| **Edit last** (off by default) | Open the last transcription in an editable bubble |
| **Command** (off by default) | Speak an instruction for the selected text |

The dictation key is **Right Ctrl** by default. Any key or combination works,
including a modifier on its own (Right Ctrl, Right Option, Right Cmd). Change
it, and turn on the optional shortcuts, in **Settings → Hotkeys**. When the
dictation key is a modifier, normal shortcuts like Ctrl+C or Option+E still
work: Murmr only reacts when you press the key by itself.

### The HUD

While you dictate, a small dark pill floats near the text field you're typing
in. It shows a recording dot, a live waveform and a timer, then a short
"transcribing" state. Once your text is typed, the pill disappears. Turn on
**Live preview** (Settings → Advanced) to see your words in the pill as you
speak.

### Speech models

Murmr ships with the fast **Base (English)** model. On a computer with a
supported GPU it downloads **Large v3 Turbo** (574 MB) in the background the
first time it opens and switches to it when ready — much better with names,
jargon and accents. Choose a model yourself in **Settings → Advanced**.

### AI cleanup and Command mode (optional)

Paste your own Anthropic API key in **Settings → Preferences** and press Save.
Murmr checks the key and turns on **AI cleanup**: Claude Haiku fixes
punctuation, grammar, false starts and lists, without changing your words. If
the service is slow or offline, Murmr types the normal text instead.

With a key set you can also bind a **Command** shortcut. Select text anywhere,
press it and say what to do ("make this shorter", "turn this into bullets",
"translate to Spanish"). With nothing selected, say what to write and it's
typed at your cursor.

The key is stored in your system keychain. Only text is sent to Anthropic.

### The main window

Open it from the tray icon (left-click, or **Open Murmr**).

- **Home** — your transcription history with search, copy, re-insert, edit
  and delete.
- **Insights** — words per minute, total words, time saved, a streak
  heatmap, most-used words and filler words.
- **Dictionary** — words, replacements and snippets, plus **Suggested**
  entries learned from your corrections.
- **Settings** — General, Microphone, Hotkeys, Preferences, Advanced, and
  Permissions (macOS).

---

## Build from source

### Prerequisites

| Tool | Why | Version |
|---|---|---|
| **Rust** (stable) | Backend | `rustc 1.75+` |
| **Node.js** | Frontend | `v20+` |
| **CMake** | whisper.cpp build | `3.20+` |
| **LLVM (clang)** | bindgen needs `libclang` | `15+` |
| **Vulkan SDK** (Windows) | GPU (Vulkan) build of whisper.cpp | `1.4.341+` |
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

# Vulkan SDK — required for the GPU build (headers, vulkan-1.lib, glslc).
# Download from https://vulkan.lunarg.com/sdk/home#windows and install, or
# copy-only into your user folder (no admin needed):
#   vulkansdk-windows-X64-<ver>.exe --root $HOME\VulkanSDK\<ver> `
#     --accept-licenses --default-answer --confirm-command install copy_only=1
```

[`scripts/run-tauri.mjs`](scripts/run-tauri.mjs) finds the VS-bundled CMake +
MSVC, the standard LLVM install path and the Vulkan SDK (`C:\VulkanSDK\*` or
`~\VulkanSDK\*`), and sets `PATH` / `LIBCLANG_PATH` / `VULKAN_SDK` so
`npm run tauri dev` works without a Developer Command Prompt.

#### One-time install on macOS

```bash
xcode-select --install
brew install cmake llvm
```

### Get the bundled models

The installer bundles two model files. The build fails without them, so
download them once into `src-tauri/models/` (they are gitignored):

```bash
mkdir -p src-tauri/models
curl -L -o src-tauri/models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
curl -fL -o src-tauri/models/ggml-silero-v5.1.2.bin \
  https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v5.1.2.bin
```

`ggml-base.en.bin` (~150 MB) is the default speech model;
`ggml-silero-v5.1.2.bin` (885 KB) detects silence. Larger models are
downloaded in-app into `<app data>/models/`.

### Run the dev build

```bash
npm install
npm run tauri dev
```

The first run takes ~5 minutes while Cargo compiles whisper.cpp and a few
hundred crates. Later runs take seconds.

### Build an app locally

```bash
# macOS, signed with your Developer ID; skips updater artifacts (no updater key needed)
APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<team>)" \
  npm run tauri -- build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The app lands in `src-tauri/target/release/bundle/macos/Murmr.app`.

### Hotkey self-test (macOS)

To check tap and hold handling without touching the keyboard, build with the
`hotkey-selftest` feature and launch with `MURMR_HOTKEY_SELFTEST=1`:

```bash
npm run tauri -- build --bundles app --features hotkey-selftest \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
open -n --env MURMR_HOTKEY_SELFTEST=1 src-tauri/target/release/bundle/macos/Murmr.app
```

It presses Right Option through macOS at several tap and hold lengths
(practice mode, so nothing is typed or saved), logs each result to
`~/Library/Application Support/app.murmr.desktop/perf.log` (`[hk-selftest]`
and `[ctrl] DictationUp … → Toggled / push-to-talk complete`), then quits.
Quit your installed Murmr first so two copies don't both react. The feature is
never enabled in release builds.

### Releasing

Bump the version in `package.json`, `src-tauri/tauri.conf.json` and
`src-tauri/Cargo.toml`, add a `CHANGELOG.md` entry, then push a tag:

```bash
git tag vX.Y.Z
git push origin main --tags
```

[`release.yml`](.github/workflows/release.yml) builds Windows, macOS Apple
Silicon and macOS Intel, signs and notarizes the Mac builds, and publishes a
GitHub Release with `latest.json` for the in-app updater. See
[DEPLOY.md](DEPLOY.md) for secrets and details.

### Project structure

```
murmr/
├── src/                       # React frontend (TypeScript)
│   ├── windows/
│   │   ├── main/              # Sidebar + pages (Home, Insights, Dictionary, Settings)
│   │   ├── hud/               # Floating dictation pill
│   │   └── onboarding/        # First-run wizard
│   ├── components/            # Shared primitives
│   ├── hooks/                 # Theme, dictation status, updater
│   └── lib/                   # ipc.ts, theme.ts, format.ts
├── src-tauri/                 # Rust backend
│   ├── src/
│   │   ├── audio/             # cpal capture + resample
│   │   ├── audio_duck/        # Lowers other audio while you dictate
│   │   ├── transcribe/        # whisper-rs, GPU detection, models, post-processing
│   │   ├── llm/               # AI cleanup + Command mode (Anthropic)
│   │   ├── hotkey/            # Global keyboard hook (tap vs hold, chords)
│   │   ├── injector/          # Clipboard paste / keystroke typing
│   │   ├── focus/             # Focused window + caret lookup (Windows, macOS)
│   │   ├── permissions/       # macOS permission checks
│   │   ├── db/                # SQLite via rusqlite
│   │   ├── learning.rs        # Dictionary suggestions from your corrections
│   │   ├── live_preview.rs    # Words in the HUD while you speak
│   │   ├── controller.rs      # State machine: hotkey → record → transcribe → type
│   │   └── lib.rs             # Tauri entry, tray, IPC handlers
│   └── models/                # Bundled models (download, gitignored)
├── vendor-rdev/               # Patched rdev (macOS 26 fixes, reliable modifier keys)
├── public/fonts/              # Bundled fonts (no Google Fonts)
└── scripts/run-tauri.mjs      # Finds CMake / MSVC / LLVM / Vulkan on Windows
```

---

## Troubleshooting

### The hotkey doesn't trigger anything

- Make sure Murmr is running (tray / menu bar icon visible) and not paused.
- Check the key in **Settings → Hotkeys**. Left and right modifiers are
  different keys: Right Ctrl won't respond to Left Ctrl.
- **macOS:** most Mac keyboards have no Right Ctrl — use Right Option. Check
  **Settings → Permissions**: Input Monitoring and Accessibility must both be
  on.
- **Windows:** a remap in PowerToys / AutoHotkey takes precedence over Murmr.

### "Microphone error: Stream error"

Your mic was disconnected mid-dictation, or another app took exclusive control
of it. Check your system's microphone privacy settings and try another input
in **Settings → Microphone**.

### "Location is not available" when opening the app data folder (Windows)

The app's identifier ends in `.desktop`, which Explorer can mistake for a file
extension. Murmr's "Open" buttons work around this; if you see it after
upgrading from an old build, restart the app.

### Transcription is slow

Check **Settings → Advanced** to see whether Murmr is using your GPU. On a
computer without one, Murmr stays on the fast Base model; avoid switching to
Large v3 Turbo there.

### Random phrases like "Thanks for watching!"

Whisper sometimes invents common phrases from near-silence. Murmr skips
silence and filters known phrases. If you still see them, move closer to the
mic or raise its input level in your system settings.

### Logs

Murmr writes a diagnostic log to `perf.log` in its app data folder:

- macOS: `~/Library/Application Support/app.murmr.desktop/perf.log`
- Windows: `%APPDATA%\app.murmr.desktop\perf.log`

---

## Roadmap

- [x] Local Whisper transcription, on the GPU when available
- [x] Global hotkey (tap / hold) + paste into any app
- [x] Floating HUD with live waveform and optional live preview
- [x] Searchable history, Insights, Dictionary with learned suggestions
- [x] First-run onboarding and macOS permissions panel
- [x] Auto-update
- [x] Signed and notarized macOS builds
- [x] AI cleanup and Command mode (bring your own Anthropic key)
- [ ] Code-signed Windows installer
- [ ] Per-app injection profiles
- [ ] Multilingual model support

---

## License

[MIT](LICENSE)
