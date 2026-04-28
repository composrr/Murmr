# Contributing to Murmr

Thanks for poking at the code. Murmr is intentionally small and self-contained
— the whole loop fits in your head once you've read the controller.

## Before you start

Read [`README.md`](README.md) → "Build from source" for the toolchain prereqs
(Rust, Node, CMake, LLVM, plus VS Build Tools on Windows or Xcode CLT on macOS).

## Where things live

- **Hot loop:** `src-tauri/src/controller.rs` is the single source of truth
  for the `idle → recording → transcribing → injected` state machine. Most
  feature work touches this file.
- **UI:** Three windows, each in `src/windows/`:
  - `main/` — sidebar shell + page router
  - `hud/` — the floating dictation pill
  - `onboarding/` — first-run wizard
  Pages communicate with the backend via `src/lib/ipc.ts` (one function per
  Tauri command).
- **Persistence:** `src-tauri/src/db/mod.rs` is the only place SQLite is
  touched. Schema lives in `db/schema/`.
- **Settings:** `src-tauri/src/settings.rs` — JSON store. New settings:
  add a field with `#[serde(default)]` so existing files keep working.

## Style

- Rust: `cargo fmt` before committing. Keep functions focused; the codebase
  prefers small files over wide ones.
- TypeScript: format with whatever your editor does (Prettier defaults are
  fine). No tooling enforced yet.
- React: function components only, hooks for state, no class components.

## Adding a Tauri IPC command

1. Write the Rust function in `src-tauri/src/lib.rs`, decorate with
   `#[tauri::command]`.
2. Add the function name to `tauri::generate_handler![...]` at the bottom of
   the file.
3. Add a typed wrapper in `src/lib/ipc.ts` so callers don't deal with
   `invoke<T>` directly.
4. The new command is callable from any window.

## Adding a settings field

1. Add the field (with a default value) to `Settings` in `src-tauri/src/settings.rs`.
2. Add the same field to the `Settings` interface in `src/lib/ipc.ts`.
3. Surface it on the relevant page (most go in `Preferences.tsx`).

## Adding a sidebar page

1. Drop the page component in `src/windows/main/pages/`.
2. Register the route in `src/windows/main/App.tsx`.
3. Add the nav item to `src/windows/main/Sidebar.tsx` — pick an icon, mind the
   top/bottom split.

## Running tests

There aren't any yet. For now, manual verification through the dev build
(`npm run tauri dev`). Test plan worth running before any PR:

- Onboarding completes (delete `%APPDATA%\app.murmr.desktop\settings.json` to
  re-trigger).
- Dictation works in Notepad (caret-positioned HUD path) and a browser
  (UIA-positioned HUD path).
- Esc cancels mid-recording.
- Shift+Right Ctrl re-pastes the last transcription.
- Toggling sounds in Preferences silences the next dictation.
- Insights numbers update after each save.

## Pull requests

Small, focused PRs. Include a short summary of what changed and why,
particularly if you touched the controller or the audio pipeline. If your
change introduces a new dependency, mention what it costs (binary size, build
time, supply-chain risk).
