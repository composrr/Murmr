# Liquid glass redesign: notes

Branch `redesign/liquid-glass`. A front-end-only redesign: no Rust engine changes, every setting and IPC call is unchanged.

## What changed

- **Look**: dark first (#09090B ground), glass panels, Inter only (no serif), one pop colour: ice blue `#7C9CFF` (`--accent` in `src/styles/globals.css`).
- **Theme**: everyone starts in Dark once (`murmr.theme.glassDefault` in `src/lib/theme.ts`). Light and Auto still work from General → Appearance.
- **Window glass**: the main window is see-through. The OS material shows behind the page:
  - Windows 11: Mica (`tauri.conf.json` → `windowEffects`).
  - macOS: `underWindowBackground` vibrancy.
  - `src/lib/windowGlass.ts` turns the see-through ground on only where the OS material exists (Windows 11+ or macOS). On Windows 10 the window stays solid.
  - The title bar follows the app theme (`setTheme`, permission `core:window:allow-set-theme`).
- **Insights**: rebuilt as the showpiece: speed hero with a weekly chart, words and time-saved tiles, a 5-stat strip, a streak heatmap, when/where charts, personal records, words, fillers, phrases and themes. Charts have hover tooltips.
- **Home**: time-of-day greeting, three glance tiles (today's words, pace, streak), glass list with the app name under each time.
- **Dictionary**: suggestion card, pill filters, glass list, and an on/off switch per entry (uses the existing `update_dictionary_entry` call).
- **Settings pages**: rows that sit next to each other join into one glass card (`.s-row` in `settings-ui.css`).
- **Sidebar**: brand mark, a live "Ready / Listening / Writing it out" card with the model and GPU/CPU.
- **Dictation pill (HUD)**: smoky glass pill, red recording dot, blue command dot. The HUD window is frameless and transparent, so the OS cannot blur the desktop behind a pill shape; the pill is a mostly-opaque glass instead.
- **Onboarding**: deep glass card, "Step X of N", accent progress bar.

## Browser preview with sample data

`npm run dev`, then open the dev URL in a normal browser (not Tauri). `src/dev/mockIpc.ts` fills the app with sample data. It only loads in dev mode outside Tauri; it is never in a build.

## NOT TESTED ON MAC

- Vibrancy behind the main window (`underWindowBackground`) and how see-through it looks.
- The title bar colour following the app theme.
- The HUD pill and onboarding card over real Mac wallpapers.

## NOT TESTED IN THE REAL APP YET (Windows)

The local Tauri build needs the Vulkan SDK, which this PC does not have. The UI was checked in a browser with sample data and through the `windows-check` CI build. Check first in the real app:

- Mica shows behind the main window, and the window is not see-through without blur.
- The HUD pill and the edit bubble are not clipped by the HUD window.
