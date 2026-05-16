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

---

## v0.1.41 — HUD never lands off-screen

### Fixed

- **HUD no longer goes invisible when its position lands outside
  every monitor.** Multi-monitor unplugs, scale-factor changes, and
  bogus focused-element coordinates from fullscreen games could
  leave the HUD "successfully shown" but positioned where the user
  couldn't see it. Murmr now verifies the HUD window overlaps at
  least one connected monitor; if not, it snaps to the
  guaranteed-visible bottom-center placement before returning.

### Improved

- **Hotkey hook telemetry in `perf.log`.** Startup logs the
  configured chords; the first keyboard event delivered to our hook
  writes a one-shot `[hotkey] first keyboard event received — hook
  is live` line; if `rdev::grab()` ever returns (which silently
  kills hotkeys), we log that too. Makes "my keyboard feels weird"
  reports diagnosable from the log alone.

---

## v0.1.40 — Audio duck recovery

### Fixed

- **Per-app volumes no longer get stranded low when Murmr stops
  mid-recording.** The audio-duck logic lowers each app's session
  volume during dictation and restores it when recording ends; if
  Murmr quit (or crashed) before the restore step ran, your game /
  Spotify / Discord stayed dimmed until you fixed it manually in
  Volume Mixer. Two new recovery paths:
  - Graceful exit (tray Quit, window close, OS shutdown) now fires
    a `RunEvent::Exit` handler that calls `unduck()` unconditionally.
  - The panic hook (added v0.1.39) also calls `unduck()` before the
    abort, so background-thread crashes restore audio too.
  The only failure mode left is killing Murmr via Task Manager —
  if that shows up in the wild we can add a force-unduck on next
  startup.

---

## v0.1.39 — Crash diagnostics

### Fixed

- Adds a panic hook so unexpected crashes on background threads
  (audio, hotkey, controller, transcribe) leave a `[panic]` line in
  `perf.log` with the thread name, source location, and message
  before the process aborts. Previously these crashes vanished
  silently. Doesn't change runtime behavior — purely diagnostic so
  the next "crashed when I pressed dictation" report has actionable
  info attached.

### Improved

- Startup `perf.log` now records the running version + target OS so
  it's obvious which build a user is on when they share a log.

---

## v0.1.38 — Mac auto-updater disabled, Mac word counter accuracy

### Fixed

- **macOS auto-updater disabled** until the app is properly
  code-signed with an Apple Developer ID. Each ad-hoc-signed rebuild
  produces a new cdhash, and macOS silently invalidates the user's
  Accessibility + Input Monitoring grants on every update — so
  every "update" was leaving Mac dictation dead until the user
  manually re-added Murmr in System Settings. Updates on Mac are now
  manual (download a fresh `.dmg` from the Releases page). Windows
  is unaffected; the auto-updater still works there.
- **Mac HUD word counter** now uses a much lower speech threshold
  (0.001 vs 0.015) to match the Mac VAD threshold. Built-in
  MacBook mics record significantly quieter than the
  Windows-tuned default; the counter wasn't ticking on normal
  speaking volume.

---

## v0.1.37 — Mac install walkthrough

### New

- **`MAC-INSTALL.md` install guide.** Murmr ships unsigned (no Apple
  Developer account behind it), so first-time Mac installs hit
  Gatekeeper's "damaged" rejection from the browser's quarantine
  flag. The new guide walks friends through the one-line `xattr`
  fix and the three macOS permissions Murmr needs (Input Monitoring,
  Microphone, Accessibility) — including which ones require
  quitting and reopening Murmr to take effect. Linked from every
  Release page so it's the first thing visible.
- **In-app Mac permissions step in onboarding.** New step (Mac-only)
  shows three cards explaining what each permission does, when
  macOS will prompt, and a one-click "Open in System Settings"
  button that deep-links to the exact pane in Privacy & Security.
  Windows users still see the existing flow with no extra step.

### Improved

- **GitHub Releases body now lists the right asset filename per
  platform** above the auto-generated commit history, so anyone
  landing on a Releases page knows immediately which `.dmg` /
  `.exe` to download.

---

## v0.1.36 — HUD post-boot recovery

### Fixed

- **HUD sometimes didn't appear when starting a recording after a
  computer restart.** Required a Murmr restart to recover. Now
  `show_hud` calls `unminimize` first (Windows can restore the
  previous session's minimized state on cold boot) and verifies the
  window is actually visible after `show()`, retrying once if not.

### Improved

- **HUD diagnostics in `perf.log`.** Every step that previously
  failed silently (window missing, positioning failures, show
  errors, `set_always_on_top` errors) now writes a single line.
  Next time the HUD misbehaves we'll know which step failed.

---

## v0.1.35 — Escape works again, word counter accuracy

### Fixed

- **Escape key passes through to the focused app when no recording is
  active.** Previously the cancel hotkey was suppressed system-wide, so
  Escape couldn't dismiss menus, dialogs, or modals in any other app
  while Murmr was running. Now Murmr only steals Escape during an
  actual recording (where it still cancels as before).
- **Word counter no longer ticks during pauses.** The HUD's speech
  threshold was below typical room noise on many setups (especially
  through VoiceMeeter and similar virtual mixers), so the counter
  climbed at a constant rate even when you weren't speaking. Threshold
  now matches the transcription gate — counter only moves when audio
  is loud enough to actually be transcribed.
- **Tap-vs-hold timing**: a 250ms intentional hold ended up looking
  like 170ms to the controller after the v0.1.33 deferred-commit
  change, sometimes parking sessions in Toggled mode unexpectedly
  (which can lead to "missing" transcriptions if you don't realize
  the recording is still going). The controller now measures elapsed
  time from the real key press, so configured tap thresholds behave
  exactly as set.

### Improved

- **Per-recording diagnostics in `perf.log`** — every recording
  writes a one-line summary (duration, sample count, peak RMS,
  whether VAD accepted it) plus state transitions. Makes "sometimes
  it doesn't post" reports diagnosable from a single log paste.

---

## v0.1.33 — Ctrl+V (and other modifier combos) work alongside dictation

### Fixed

- **Bare-modifier dictation hotkeys (default Right Ctrl) no longer
  break combo shortcuts in other apps.** Pressing Ctrl+V to paste
  used to come through as just `V` because Murmr was eating the
  Ctrl press to fire dictation. Now when the dictation hotkey is a
  bare modifier (Ctrl/Shift/Alt/Meta), the press passes through to
  the focused app, and "start dictation" is held back for ~80ms — if
  any non-modifier key arrives in that window, it was a combo and
  no recording starts. If 80ms passes with no other key, recording
  starts as normal. Push-to-talk loses 80ms at the very start, which
  is imperceptible. Non-modifier hotkeys (F8, letters, symbols)
  keep the original suppress-on-press behavior.

---

## v0.1.32 — Smarter numbered-list detection

### New

- **Ordinals work as list markers.** "First, ... second, ... third, ..."
  formats as a numbered list, same as "one, two, three". Both word
  forms (first–twentieth) and numeric forms (1st, 2nd, 3rd, 4th) are
  recognized.
- **List-intent detection.** When the surrounding text contains
  setup phrases like "here are…", "the following…", "let me list…",
  "two things…", "three reasons…", "a few options…", Murmr is more
  willing to format an enumeration even if the markers don't start
  at 1 ("two… three…") or skip a number ("first… third…"). Without
  intent it stays strict to avoid corrupting prose like "page one…
  page three".

### Improved

- Connector words (`and`, `or`, `for`, `step`, `item`, `point`,
  `reason`, …) between punctuation and a marker no longer block the
  match — "...do this. **And two**, do that." formats correctly.
- Output always renumbers cleanly from 1, regardless of which
  markers the speaker actually used.

---

## v0.1.27 — Custom start / stop sounds + volume control

### New

- **Bundled start and stop chimes** for the press / release of the
  dictation hotkey. Distinct, short, and tuned to be audible
  without dominating whatever you're listening to.
- **Sound volume slider** in Settings → Audio. Defaults to 70%.
  Set to 0 to mute Murmr's own sounds entirely.
- Drop a custom `start.wav` or `stop.wav` into
  `<app-data>/sounds/` to override the bundled chimes.

### Improved

- Mac sound playback fixed — was using a fixed-duration timer that
  raced CoreAudio init on cold launch, so the first chime of a
  session sometimes never reached the speakers. Now waits for the
  audio sink to actually drain.

---

## v0.1.23 — Murmr is free

### New

- **Murmr is now free for anyone.** No license key, no paywall, no signup.
  The whole licensing surface (paywall screen, key generator, Ed25519
  validation, settings field) is gone. Your existing license keys keep
  installing fine but they aren't checked anymore.

### Improved

- Smaller binary — dropped `ed25519-dalek` + `base64` dependencies along
  with the license module.
- Faster cold start — no license-check on every dictation press cycle.

---

## v0.1.22 — macOS 26 fixes

### Fixed

- **macOS 26 first-keystroke crash** — vendored `rdev` to skip the
  `TSMGetInputSourceProperty` call inside its `CGEventTap` callback,
  which was firing `dispatch_assert_queue` on a non-main thread and
  killing the app the moment any key was pressed.
- macOS focus detection (`focus/macos.rs`) — reads the focused text
  field's AXUIElement so the HUD lands near the input.
- macOS VAD threshold lowered from 0.015 → 0.001 (MacBook built-in mics
  record significantly quieter than the headset/desktop mics this was
  originally tuned for).
- Transparent HUD on macOS via `tauri/macos-private-api` feature.

---

## v0.1.21 — Tagged correctly

### Fixed

- Version mismatch from v0.1.20: tag landed on a commit where source still
  said "0.1.18" so CI built and uploaded mismatched binaries.
- `release-beta.mjs` now auto-commits the version bump before you can tag,
  so the source version always matches the tag.

---

## v0.1.20 — In-app changelog (BROKEN — withdrawn)

This release was published then immediately withdrawn — Windows installers
were misnamed and would have caused an OTA update loop. Use v0.1.21 or
later. (Notes preserved for completeness.)

### New

- README.txt ships next to the installed app (Windows install dir or
  macOS .app/Contents/Resources). Plain English: hotkeys, settings tour,
  privacy policy, troubleshooting.
- Updater re-checks every 6 hours during a long-running session, in
  addition to the launch-time check.
- System toast notification when an update is detected (Windows / macOS
  native), so users in tray-only mode see updates without opening the app.
- In-app **What's new** modal — fetches `CHANGELOG.md` from GitHub and
  renders it brand-styled. Open from the update banner or
  Settings → General → Release notes.
- Self-contained styled HTML changelog page at `tools/changelog-page.html`
  for the future marketing site.

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
