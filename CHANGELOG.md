# Changelog

All notable changes to Murmr land here. The most recent version is at the
top. We follow [Semantic Versioning](https://semver.org) — pre-1.0 means
breaking changes can land in a minor bump, but we'll always note them.

This file is the source of truth for the in-app "What's new" view AND the
public release notes website. Keep entries short, user-facing, and
grouped by `New / Improved / Fixed`.

---

## [Unreleased]

_Anything currently in `main` that hasn't been tagged yet lands here._

### New

- README.txt now ships next to the installed app (Windows install dir or
  macOS .app/Contents/Resources). Plain English: hotkeys, settings tour,
  privacy policy, troubleshooting.
- Updater also re-checks every 6 hours during a long-running session, in
  addition to the launch-time check. Long-lived Murmr instances catch new
  versions without needing a relaunch.

---

## v0.1.17 — 2026-04-29

### Improved

- **Chord shortcuts** (`Ctrl+Shift+V`-style) now work on every hotkey row.
  Capture state machine waits for either a non-modifier key (combo bind)
  or a modifier-release without intervening key (bare-modifier bind).
- **Audio ducking defaults** bumped: starts at 80% reduction (was 30%) and
  the slider now goes to 95% (was 70%) so dimming is unmistakable.

### Fixed

- Hotkey capture used to fire on the FIRST key pressed, so binding to
  `Ctrl+V` always saved as `ControlLeft` because Ctrl came first.

## v0.1.16 — 2026-04-29

### Improved

- Duck slider max bumped from 70% → 95%.

## v0.1.15 — 2026-04-29

### New

- **Chord shortcut support** (initial). Hotkey rows now accept any
  combination of 0–N modifiers plus one main key. Strict matching: extra
  modifiers held disqualify the chord (so `Shift+V` doesn't fire on
  `Ctrl+Shift+V`).
- **Per-app audio session ducking** via `IAudioSessionManager2`. Walks
  every active render device + every app session and dims each individually.
  Works regardless of Voicemeeter / virtual cables / Bluetooth routing —
  the previous master-endpoint approach only touched the default device.

### Fixed

- Misleading "Phase 9" toggles removed (noise suppression, GPU backend).
  They were UI promises we never wired to any backend.

## v0.1.14 — 2026-04-29

### Improved

- Internal: build wrapper honors `CARGO_TARGET_DIR` so we can dodge
  Windows Defender file-locks during dev.

## v0.1.13 — 2026-04-29

### Fixed

- Updater UX: routine "couldn't reach update server" failures no longer
  trigger a red error banner. Only actual install failures (signature
  mismatch, network drop mid-download) get the loud treatment.

## v0.1.12 — 2026-04-29

### New

- **Audio ducking** (initial). Lowers system master volume by a
  configurable amount while you're dictating; restores when done.

### Fixed

- Misleading "phase 9" labels in the UI replaced with honest copy.

## v0.1.11 — 2026-04-29 — first public release

### New

- **Cross-platform CI**: tag a version → GitHub Actions builds Windows
  installer + macOS DMG (M-series + Intel) + a unified `latest.json`
  that the auto-updater consumes. One tag, all platforms.
- **Browser-based key generator** (`tools/key-generator.html`) — paste
  recipient's email, click Generate, copy the license key. Self-contained
  HTML, no server needed.

## v0.1.10 — 2026-04-28

### Improved

- All hotkey labels in the UI (Home subtitle, Settings status, onboarding
  cards) now reflect the user's actually configured key, not hardcoded
  "Right Ctrl".

## v0.1.9 — 2026-04-28

### Fixed

- "Install now" button in Settings → General was wired to checkNow not
  installNow — clicking it just re-checked instead of installing.
- Banner + Settings page now share a single updater hook instance via
  React context, so a check from Settings updates the banner and vice
  versa.

## v0.1.7 — 2026-04-28

### Improved

- **Re-paste shortcut is now its own independent hotkey** — used to be
  tied to "Shift + (dictation key)". Now bind it to anything you want:
  F9, `]`, etc. Or leave empty to disable.

## v0.1.6 — 2026-04-28

### Fixed

- Removed redundant "Shortcut: Right Ctrl" row from Settings → General
  (the same info lives on the Hotkeys page).

## v0.1.5 — 2026-04-28

### New

- **License-key gating**. Murmr requires an Ed25519-signed license key on
  first launch. Paywall screen with paste-and-activate flow.
- **Hotkey suppression** — pressing your bound dictation key (e.g. `~`)
  no longer leaks the character into the focused app.
- **Devtools** enabled (Ctrl+Shift+I) for beta debugging.

## v0.1.0 — 2026-04-27 — initial Windows-only beta

- Local Whisper transcription with the base.en model
- Tap / hold global hotkey (default Right Ctrl)
- Voice commands ("period", "comma", "new line", etc.)
- Filler-word stripping
- SQLite-backed history with full-text search
- Tauri 2 auto-updater with localhost beta loop
