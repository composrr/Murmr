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

## v0.1.60 — HUD display toggles actually work (word count now optional)

### Fixed

- **The HUD display toggles in Preferences now do something.** "Show
  waveform," "Show timer," and "Show estimated word count" have
  existed in Settings for a while but the recording pill ignored
  them and always rendered all three. They're now wired up — flip
  any of them and the pill updates the next time it appears.

### Changed

- **The live word count is now off by default.** It's a time-based
  estimate (no streaming transcription), so it's more of a fun stat
  than a fact — the recording pill is cleaner as just waveform +
  timer. Want it back? Settings → Preferences → HUD → "Show
  estimated word count." Your exact word counts in History and
  Insights are unchanged. (If you already had it on, it stays on
  until you turn it off.)

---

## v0.1.59 — Live word counter reads words, not syllables

### Fixed

- **The live word counter in the recording pill no longer runs high.**
  While recording, Murmr can't know the real word count yet (there's
  no streaming transcription), so the pill estimates it from how long
  you've been actively speaking. The assumed rate was 220 WPM —
  auctioneer-fast — which made the estimate run about 1.4× high. Since
  English averages ~1.4 syllables per word, that made the count look
  like it was tallying syllables. The rate is now 150 WPM (a realistic
  dictation average), so the live estimate tracks actual words much
  more closely. (The saved word counts in History and Insights were
  always exact — computed from the real transcript — and are
  unchanged.)

---

## v0.1.58 — Game audio actually comes back after dictating

### Fixed

- **Per-app volumes restore correctly on multi-device / multi-bus
  audio setups** (Voicemeeter, games with master + sub-mix sessions
  like Apex Legends). When one process has several audio sessions at
  different volumes, the saved "original" used to be whichever
  session happened to enumerate last — often a quiet sub-bus — so
  restoring dragged the loud master down to it. We now save the MAX
  across each process's sessions, so restore never lands below any
  session's true original. (This shipped briefly in v0.1.52 and was
  reverted after a test that turned out to be invalidated by the
  AVX-512 crash happening at the same time. The crash is fixed, the
  fix is back.)
- **Sessions that vanish mid-dictation get restored when they
  reappear.** Fullscreen games tear down and recreate audio sessions
  on scene transitions and alt-tabs — if the session we ducked didn't
  exist at the moment dictation ended, we used to give up, and
  Windows *persists* that ducked per-app volume, leaving the game
  quiet until you fixed the mixer by hand. Unduck now retries at
  +2s / +10s / +30s, catching the session as it comes back.

---

## v0.1.57 — Only one Murmr at a time

### Fixed

- **Murmr now enforces a single running instance.** If a second
  copy launches (e.g. an autostart entry plus a manual launch), it
  hands off to the already-running one and exits instead of starting
  a competing process. Two instances meant two global keyboard hooks
  AND two independent audio-duck managers fighting over your per-app
  volumes — one would duck, the other would duck again over the
  already-lowered value and save *that* as the "original," so on
  release your audio never came back. That whole class of
  duck-never-returns weirdness from an accidental double-launch is
  gone. Re-launching while Murmr is already running now just surfaces
  the existing window.

---

## v0.1.56 — Pause Murmr in fullscreen apps (games, etc)

### New

- **Murmr now ignores the dictation hotkey while a fullscreen app
  is focused.** Three wins from one setting:
  1. **No more stuck state in games** — the v0.1.55 recovery
     path catches it after the fact; this prevents it from
     happening in the first place. The hotkey just passes
     through, no recording starts, no audio ducks.
  2. **No accidental in-game triggers** — your dictation key is
     probably also a key you use in-game. Murmr now stays out
     of the way until you alt-tab.
  3. **Less anti-cheat surface** — anti-cheat systems
     (EasyAntiCheat, Vanguard, BattlEye) get nervous about apps
     that act on global keyboard hooks during competitive
     gameplay. While the hook itself is still installed (so we
     can resume the instant you alt-tab), we no longer fire
     dictation events while a fullscreen game has focus.
  Toggle is at **Settings → Preferences → Games & fullscreen
  apps** if you specifically want Murmr to dictate inside a
  fullscreen app — e.g. note-taking in a fullscreen browser.
  Default ON.

---

## v0.1.55 — Recover from fullscreen-game-ate-the-key-release

### Fixed

- **Murmr no longer gets stuck after a fullscreen game (Apex
  Legends, etc) eats the dictation key release.** Previously the
  sequence — press hotkey while Apex focused → press registers and
  starts recording + ducks audio → game eats the release → controller
  stays in `HoldUncertain` forever — left audio ducked indefinitely
  AND made the hotkey appear "dead" (subsequent presses silently
  no-op'd). The only recovery was quitting Murmr. Two fixes:
  - **Press again to recover** — a second `DictationDown` while in
    `HoldUncertain` now treats the prior recording as complete
    (transcribes whatever was captured, unducks audio, returns to
    Idle) instead of being ignored. So if you notice the stuck
    state, alt-tab out of the game and tap your hotkey one more
    time; everything resets.
  - **Walk-away watchdog** — every recording also spawns a quiet
    10-minute timer; if the recording is still active when it
    fires (which only happens if release was lost and the user
    didn't press again), it force-unducks audio so other apps
    don't sit at half volume waiting for you. Sessioned so a
    stale watchdog can't fire into a fresh recording.

---

## v0.1.54 — Fix illegal-instruction crash on transcribe (disable AVX-512)

### Fixed

- **Murmr no longer crashes during transcription** on machines
  whose CPUs trip on whisper.cpp's AVX-512 codegen. The reported
  symptom: Murmr would die silently right after the start chime
  ("after translating," `STATUS_ILLEGAL_INSTRUCTION` in Windows
  Event Log). Whisper builds with AVX-512 enabled emit at least
  one instruction that some AVX-512-capable CPUs reject at the
  microarchitecture level even though cpuid claims support.
- Build now forces `GGML_NATIVE=OFF` and disables every
  `GGML_AVX512*` codegen path in whisper.cpp's CMake, both in
  `scripts/release-beta.mjs` (local builds) and
  `.github/workflows/release.yml` (CI / OTA releases). Trade-off:
  on real AVX-512-capable Intel Xeons we leave a bit of inference
  throughput on the table, but every CPU now produces correct
  results without crashing. AVX2 is still enabled — performance
  on consumer chips (AMD Zen 2/3/4/5, Intel non-server) is
  effectively unchanged.

---

## v0.1.53 — Diagnostics: full hotkey-chord logging

### Improved

- The startup `[hotkey] installing OS keyboard hook` log line in
  `perf.log` now shows the FULL chord (modifiers + main key) for
  each binding, not just the main key. Lines used to read
  `dictation=MetaLeft, repeat=Some(KeyV)` (ambiguous — could be
  bare keys or modifier combos) and now read
  `dictation=Ctrl+MetaLeft, repeat=Ctrl+Shift+KeyV` (or
  `<bare> KeyV` if it really is just bare V). Makes "my hotkey
  is doing weird stuff" reports diagnosable from one log line.

### Reverted

- The v0.1.52 audio-duck MAX-per-PID heuristic — didn't actually
  help the Apex Legends "audio stays quieter after dictation"
  case the user reported. Behavior is back to v0.1.51's PID-keyed
  last-wins restore. Investigation continues with better logs.

---

## v0.1.51 — Audit pass: HUD reliability + log resilience + chord ordering

### Fixed

- **HUD now appears reliably on cold launch and after long idle.**
  Three layered race conditions resolved: (1) the React listener
  for status events is now attached BEFORE the on-mount
  `is_recording_active()` query, eliminating the gap where live
  events could fire into the void; (2) the timed `Status::Recording`
  re-emit threads now carry a session ID and silently skip if a
  newer recording has started OR the current one has ended, so
  stale events can't clobber the HUD's view; (3) the controller
  fires a new `murmr:hud-shown` event after every `show_hud()`
  which the HUD listens for and self-heals on receipt — a sharper
  signal than timed re-emits for the WebView-suspended-during-emit
  case.
- **Modifier+modifier hotkeys (e.g. `Ctrl+Win`) now match
  order-independently.** Previously the chord required the user to
  press the chord-prefix modifier FIRST and the main key SECOND;
  pressing them in the other order silently missed. Now the chord
  fires as soon as ALL required keys are simultaneously held —
  Ctrl-then-Win, Win-then-Ctrl, and both-at-once all work. Release
  of EITHER key ends the recording. Bare modifier chords and
  modifier+letter chords keep their original order-dependent
  matching (letters are the natural trigger; bare modifiers need
  L/R specificity).

### Improved

- **`perf.log` dual-write.** Every line now writes to BOTH the
  primary `<app_data>/perf.log` AND a fallback `<exe_dir>/perf.log`
  next to `murmr.exe`. If one path silently fails (permissions,
  missing env var, Windows integrity-level quirks), the other
  still captures the diagnostic trail — no more "the app is
  running but logs vanish" mystery.
- **`resolve_app_data_dir` fallbacks for Windows.** When `%APPDATA%`
  isn't propagated by the launching process (which can happen with
  some Explorer/shortcut launch contexts), we now construct the
  Roaming path from `%USERPROFILE%` → `%LOCALAPPDATA%` → exe-
  adjacent, rather than silently falling through to
  `current_dir()` and writing settings/DB to the wrong place.
- **Startup env diagnostics in `perf.log`.** New
  `[startup] env present: APPDATA=... USERPROFILE=...` line so
  future launch-context puzzles are diagnosable from a single
  log paste.
- **Listener cleanup.** HUD listener unregistration is now driven
  off awaited promises, so rapid mount/unmount cycles (HUD
  recreation, etc) can't leak event handlers anymore.

---

## v0.1.50 — Modifier+modifier hotkeys stop blocking system shortcuts

### Fixed

- **Binding dictation to a modifier+modifier chord (e.g.
  `Ctrl+Win`) no longer breaks Windows system shortcuts** like
  `Win+Ctrl+D` (new virtual desktop) or `Win+Ctrl+arrows` (switch
  desktop). v0.1.49 enabled binding to those chords, but the
  hotkey hook was still eating the Win press the instant Ctrl was
  held. The deferred-commit + pass-through path that already
  existed for bare-modifier hotkeys now covers *any* chord whose
  main key is a modifier — the modifier flows through to the OS
  for 80ms, and only commits to dictation if no third key arrives
  in that window. So `Ctrl+Win+D` still creates a virtual desktop,
  but holding `Ctrl+Win` alone for >80ms starts dictation.

---

## v0.1.49 — Bind any combination, including modifier+modifier

### New

- **Hotkey capture now accepts modifier-only combinations** like
  Ctrl+Win, Alt+Shift, Ctrl+Alt, etc. Previously the chip required a
  non-modifier "main key" or a single bare modifier. Now: press
  whichever modifiers you want in any order and release them — the
  last-pressed modifier becomes the "main key" of the chord and the
  earlier ones become its modifiers. So pressing Ctrl→Win→release
  binds to `Ctrl+MetaLeft`. The Rust matching layer already supported
  this; the UI was the gate.

---

## v0.1.48 — Post-transcribe diagnostics

### Improved

- Added breadcrumb logs in `perf.log` along the entire post-transcribe
  path (after Whisper, after postprocess, before/after inject, before/
  after DB insert, before/after notification scheduling). Whichever
  line is last in the log when a crash happens points at the
  step that died.
- Wrapped the milestone notification flow in `catch_unwind` on both
  sides (controller dispatch + the spawned worker thread) so a panic
  in that code path is logged and the app survives.

---

## v0.1.47 — Stop crashing on dictation

### Fixed

- **Murmr no longer crashes mid-dictation.** v0.1.46 had a hard
  native-code crash (STATUS_ILLEGAL_INSTRUCTION) on every dictation
  attempt for some users. Root cause was unsafe PWSTR / COM handling
  in the per-session audio-duck code added in v0.1.44. Reverted that
  change — audio ducking goes back to per-process keying (slightly
  imperfect restore for apps with multiple audio sessions per PID like
  Chrome / Discord, but stable). Also wrapped the entire audio-duck
  flow in `catch_unwind` so any future bug in that area can never
  crash the whole app again — a failed-to-duck is now logged and
  Murmr proceeds rather than aborting.

---

## v0.1.46 — Fast "Check for updates"

### Fixed

- **"Check for updates" in Settings is now near-instant** instead of
  taking 5–10 seconds. The shipping config had a local dev endpoint
  (`http://127.0.0.1:8123`) listed *before* the public GitHub one —
  used during local OTA testing, but users have nothing listening on
  that port. Every check was waiting for the local connection to
  refuse before falling through. Public config now points at GitHub
  only; local OTA testing is still available via direct install from
  `release-staging/`.

---

## v0.1.45 — HUD self-heals on cold launch + wake-from-idle

### Fixed

- **HUD now appears reliably even when first opening Murmr or
  returning after a long idle.** You'd previously hear the start
  chime but see no pill — the React app's listener for status
  events sometimes wasn't ready when the first event fired (cold
  launch race) or had been suspended by the OS (WebView sleeps
  background windows after long idle). Two-layer fix: the
  controller now re-emits `Status::Recording` at +120ms and +500ms
  after the initial emit (idempotent — duplicate events are
  no-ops), AND the HUD on mount asks Rust "are you currently
  recording?" so it can self-heal if it missed the live event
  entirely.

---

## v0.1.44 — Audio duck no longer strands per-app volumes

### Fixed

- **Per-app volumes now return fully to where they were** when a
  dictation ends. Apps with multiple audio sessions per process
  (Chrome's per-tab streams, Discord's per-channel streams, OBS, Slack
  notifications + main, etc.) were getting their volumes saved by
  process ID, which meant the second session's value overwrote the
  first's in our internal map — so on restore all of that app's
  sessions snapped to whichever single value won the race. Volumes are
  now saved + restored per audio session identifier, so each session's
  true original volume is preserved.

### Improved

- `perf.log` after each unduck now reports
  `restored X/Y session(s) (Z missing — process/session ended)` so
  any further volume-stranding can be diagnosed from the log alone.

---

## v0.1.43 — Insights expansion + milestone notifications

### New

- **Trends section on the Insights page** with four new cards:
  - **Speaking pace** — 12-week sparkline of your weekly average WPM,
    with the delta vs the prior week called out next to the title.
  - **Personal records** — three stat tiles for your all-time longest
    dictation (by word count), longest by duration, and highest-WPM
    session (50-word minimum so a quick 2-word burst doesn't dominate).
  - **Filler progress** — month-over-month change in your top filler
    ("um", "uh", etc.) — "38% less this month than last."
  - **Where you dictate** — quiet, understated bar list of the top
    apps you use Murmr in.
- **Milestone notifications** (toggleable in Settings → Preferences →
  Notifications). Rare-and-meaningful celebratory pop-ups: 1st / 100th
  / 500th / 1000th / 5000th transcription, 10k / 100k / 1M words, 7 /
  30 / 100-day streaks, and (throttled to once per week) a new
  personal best on dictation length or WPM. Fires 4 seconds after a
  successful inject so the notification doesn't compete with the text
  the user just pasted; suppressed entirely when the focused window is
  fullscreen-sized (probable game / video / DnD context).

### Improved

- New `filler_events` time-indexed log alongside the existing
  cumulative `filler_counts` — lets the Insights page answer windowed
  questions like "did you say 'um' less this month" without breaking
  the existing filler card.

---

## v0.1.42 — Mac dock-free + Windows HUD post-boot recovery

### Fixed

- **Windows HUD now works on the first dictation after a system
  reboot.** When Murmr auto-starts during a cold boot, the WebView2
  runtime races with the HUD window creation, and Tauri's startup
  sometimes lost. Previously the user had to quit + relaunch Murmr to
  recover. Now Murmr recreates the HUD via `WebviewWindowBuilder` on
  the spot the first time it tries to show it and finds it missing.

- **Mac: Murmr no longer appears in the Dock.** It now runs as a true
  accessory app (`LSUIElement` = true) — menu-bar-only, no Dock icon,
  no application menu. Matches how WhisperFlow, Maccy, Rectangle, etc.
  behave on macOS. Closing the main window hides it instead of
  quitting; use the menu-bar tray's "Quit Murmr" to actually quit.

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
