Murmr — Privacy-first local voice dictation
============================================

Murmr listens to your microphone, transcribes what you said using a local
copy of OpenAI's Whisper model, and pastes the text into whatever you were
typing. Audio NEVER leaves your machine — no cloud, no telemetry.


First run
---------

1. Murmr launches the main window — no signup, no key, no account. It's
   free.
2. Murmr lives in your system tray (taskbar bottom-right). Closing the
   main window keeps it running quietly.
3. Default hotkey: TAP "Right Ctrl" to start dictating, TAP again to stop.
   HOLD "Right Ctrl" for push-to-talk (release to stop).


How dictation works
-------------------

  Tap   →  Recording starts immediately. Tap again to stop.
  Hold  →  Push-to-talk. Recording stops the moment you let go.
  Esc   →  Cancel a recording in progress (no transcription, no insert).


Settings (open from the system tray icon → "Open Murmr")
-------------------------------------------------------

General      Display name, launch-at-login, "Check for updates"
Microphone   Input device, gain, audio-ducking (dims background apps)
Hotkeys      Rebind dictation / re-paste / cancel keys (any combo: F8,
             Ctrl+Shift+V, Right Ctrl alone, etc.)
Preferences  Voice commands ("period", "comma", "new line"), filler-word
             stripping, sound effects, HUD options
Dictionary   Custom replacements / proper nouns Whisper should remember
Insights     History of every transcription, search, top words, hourly use
Advanced     File paths, log level, raw model info


Updates
-------

Murmr checks for new versions automatically — once at launch and again every
6 hours. When an update is available, a banner appears at the top of the
main window with one-click install. You can also force a check from
Settings → General → "Check for updates".


Permissions
-----------

  Windows  No special permissions. The installer writes to your user folder
           (%LOCALAPPDATA%\Murmr) so admin rights are NOT needed.

  macOS    First launch will request:
            - Microphone access (system prompt)
            - Accessibility (manual: System Settings → Privacy &
              Security → Accessibility → toggle Murmr ON)
            - Input Monitoring (manual: same panel as above)
           Without these, Murmr can't capture audio or paste text.


Privacy
-------

Everything stays on your machine. There is no analytics, no usage telemetry,
no cloud transcription service. The only network call Murmr makes is to
GitHub, to check for new releases. You can disable that by toggling the
endpoint off (Settings → General).


Trouble?
--------

  - Dictation isn't triggering: check Settings → Hotkeys; your bound key
    might conflict with another running app's shortcut.
  - Transcription is slow: open Settings → Advanced and check that the
    Whisper model is `ggml-base.en.bin`. The first run is slower because
    the model has to load (~2 sec) — subsequent runs are sub-second.
  - macOS pastes wrong app: turn on accessibility permissions (above) AND
    make sure the target app has focus when you stop dictating.


Source
------

Murmr is free for anyone. Public source at:
https://github.com/composrr/Murmr
