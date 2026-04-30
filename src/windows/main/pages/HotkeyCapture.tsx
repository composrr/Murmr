import { useEffect, useRef, useState } from 'react';

/**
 * Click-to-capture hotkey input. The user clicks the chip; we listen for the
 * next physical key (excluding the modifier keys themselves unless the user
 * specifically wants to bind a bare modifier — that's the common case for
 * dictation hotkeys, hence the `allowBareModifiers` prop).
 *
 * Returns the rdev::Key debug name (e.g. "ControlRight", "F8", "Escape") via
 * `onChange` so it round-trips with the backend `Settings` struct.
 */

interface Props {
  value: string;
  onChange: (next: string) => void;
  /** When true, allow capturing a bare modifier press (no following key).
   * Used for the main dictation key. The cancel key has this off so the
   * user can't accidentally bind it to e.g. just Shift. */
  allowBareModifiers?: boolean;
  /** Optional list of keys we refuse to bind to (e.g. the cancel key
   * shouldn't be bindable as the dictation key). */
  forbidden?: string[];
  disabled?: boolean;
}

export default function HotkeyCapture({
  value,
  onChange,
  allowBareModifiers = false,
  forbidden,
  disabled,
}: Props) {
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!capturing) return;

    // State: which modifiers are currently held + the rdev name of the FIRST
    // modifier pressed. We use this to support "press a single bare modifier
    // and release it" → bind to that modifier. If the user adds a non-
    // modifier key BEFORE releasing, we capture the combo on that keydown.
    const heldModifiers = new Set<string>();
    let firstModifierRdev: string | null = null;
    let captured = false;

    function isModifierCode(code: string): boolean {
      return (
        code === 'ControlLeft' || code === 'ControlRight' ||
        code === 'ShiftLeft' || code === 'ShiftRight' ||
        code === 'AltLeft' || code === 'AltRight' ||
        code === 'MetaLeft' || code === 'MetaRight' ||
        code === 'OSLeft' || code === 'OSRight'
      );
    }

    function handleKeyDown(e: KeyboardEvent) {
      e.preventDefault();
      e.stopPropagation();
      if (captured) return;

      // Esc with no modifiers cancels capture (so users who want Esc as
      // their cancel key just press it; users who want Shift+Esc as a
      // combo can do that — the modifier disqualifies the cancel path).
      const noMods = !e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;
      if (e.key === 'Escape' && noMods && heldModifiers.size === 0) {
        captured = true;
        setCapturing(false);
        setError(null);
        return;
      }

      if (isModifierCode(e.code)) {
        // Track the modifier as held. Don't capture YET — wait to see if
        // a non-modifier key comes next (combo) or if the user just
        // releases the modifier (bare-modifier bind).
        heldModifiers.add(e.code);
        if (firstModifierRdev === null) {
          firstModifierRdev = browserKeyToRdev(e);
        }
        return;
      }

      // Non-modifier keydown → COMBO capture.
      const rdevName = browserKeyToRdev(e);
      if (!rdevName) {
        const codeHint = e.code ? ` (browser saw code="${e.code}")` : '';
        setError(`"${e.key}" isn't bindable yet${codeHint}. Try a letter, digit, function key, modifier, or symbol.`);
        return;
      }

      // Modifiers in canonical order: Ctrl, Shift, Alt, Meta.
      const parts: string[] = [];
      if (e.ctrlKey) parts.push('Ctrl');
      if (e.shiftKey) parts.push('Shift');
      if (e.altKey) parts.push('Alt');
      if (e.metaKey) parts.push('Meta');
      parts.push(rdevName);
      const chord = parts.join('+');

      if (forbidden?.includes(chord)) {
        setError('That combination is already used by another shortcut.');
        return;
      }

      captured = true;
      onChange(chord);
      setCapturing(false);
      setError(null);
    }

    function handleKeyUp(e: KeyboardEvent) {
      if (captured) return;
      if (!isModifierCode(e.code)) return;

      heldModifiers.delete(e.code);

      // All modifiers released without a non-modifier in between → bare-
      // modifier bind (only honored on rows that opted in).
      if (heldModifiers.size === 0 && firstModifierRdev !== null && allowBareModifiers) {
        const chord = firstModifierRdev;
        if (forbidden?.includes(chord)) {
          setError('That key is already used by another shortcut.');
          firstModifierRdev = null;
          return;
        }
        captured = true;
        onChange(chord);
        setCapturing(false);
        setError(null);
      }
      // If allowBareModifiers is false and they released without a key,
      // just reset state and let them try again.
      if (heldModifiers.size === 0) {
        firstModifierRdev = null;
      }
    }

    window.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('keyup', handleKeyUp, true);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      window.removeEventListener('keyup', handleKeyUp, true);
    };
  }, [capturing, allowBareModifiers, forbidden, onChange]);

  // Re-focus the button when entering capture so subsequent blur cancels it.
  useEffect(() => {
    if (capturing) buttonRef.current?.focus();
  }, [capturing]);

  return (
    <div className="flex flex-col items-end gap-1 w-[260px]">
      <button
        ref={buttonRef}
        type="button"
        disabled={disabled}
        onClick={() => {
          if (disabled) return;
          setCapturing((c) => !c);
          setError(null);
        }}
        onBlur={() => {
          // Defer so a click on a same-row button doesn't immediately cancel.
          setTimeout(() => setCapturing(false), 100);
        }}
        className={[
          'w-full rounded-[8px] border px-3 py-[6px] text-[12px] font-medium transition-colors',
          capturing
            ? 'border-[#7a7a72] bg-bg-row text-text-primary animate-pulse'
            : 'border-border-control bg-bg-control text-text-primary hover:border-[#7a7a72]',
          disabled && 'opacity-50 cursor-not-allowed',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        {capturing ? 'Press a key or combo…  (Esc to cancel)' : displayName(value)}
      </button>
      {error && (
        <span className="text-[11px] text-[#c14a2b] dark:text-[#e87a5e] leading-tight">{error}</span>
      )}
    </div>
  );
}

/** Render a chord (`Ctrl+Shift+KeyV`) or single key (`F8`, `ControlRight`)
 *  with friendly labels for each part, joined by ` + `. */
export function displayName(chord: string): string {
  if (!chord) return '';
  const parts = chord.split('+').map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return chord;
  return parts.map(displaySinglePart).join(' + ');
}

function displaySinglePart(name: string): string {
  if (KEY_LABELS[name]) return KEY_LABELS[name];
  // Bare modifier names (chord prefixes)
  if (name === 'Ctrl' || name === 'Shift' || name === 'Alt') return name;
  if (name === 'Meta') return 'Cmd / Win';
  // Letters: KeyA → A
  const letterMatch = /^Key([A-Z])$/.exec(name);
  if (letterMatch) return letterMatch[1];
  // Top-row digits: Num0 → 0
  const digitMatch = /^Num(\d)$/.exec(name);
  if (digitMatch) return digitMatch[1];
  // Numpad: Kp5 → Numpad 5
  const kpMatch = /^Kp(\d)$/.exec(name);
  if (kpMatch) return `Numpad ${kpMatch[1]}`;
  return name;
}

const KEY_LABELS: Record<string, string> = {
  // Modifiers
  ControlLeft: 'Left Ctrl',
  ControlRight: 'Right Ctrl',
  ShiftLeft: 'Left Shift',
  ShiftRight: 'Right Shift',
  Alt: 'Alt',
  AltGr: 'Right Alt',
  MetaLeft: 'Left Cmd / Win',
  MetaRight: 'Right Cmd / Win',
  CapsLock: 'Caps Lock',
  // Nav + control
  Escape: 'Esc',
  Return: 'Enter',
  Space: 'Space',
  Tab: 'Tab',
  Backspace: 'Backspace',
  Delete: 'Delete',
  Insert: 'Insert',
  Home: 'Home',
  End: 'End',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
  UpArrow: '↑',
  DownArrow: '↓',
  LeftArrow: '←',
  RightArrow: '→',
  PrintScreen: 'Print Screen',
  ScrollLock: 'Scroll Lock',
  Pause: 'Pause',
  NumLock: 'Num Lock',
  // Symbols
  BackQuote: '` (backtick)',
  Minus: '- (minus)',
  Equal: '= (equals)',
  LeftBracket: '[',
  RightBracket: ']',
  BackSlash: '\\',
  SemiColon: ';',
  Quote: "'",
  Comma: ',',
  Dot: '.',
  Slash: '/',
  IntlBackslash: '\\ (intl)',
  // Numpad helpers (digits handled by regex above)
  KpReturn: 'Numpad Enter',
  KpMinus: 'Numpad −',
  KpPlus: 'Numpad +',
  KpMultiply: 'Numpad ×',
  KpDivide: 'Numpad ÷',
  KpDelete: 'Numpad .',
};

// (isBareModifier helper removed — modifier-only chords are now built in
// the chord-construction logic above.)

/**
 * Translate a browser KeyboardEvent into the rdev::Key debug name our backend
 * uses. Returns null for keys we don't yet support binding.
 *
 * The browser uses two relevant fields:
 *  - `code`: physical-key identifier ("ControlRight", "Space", "F8").
 *  - `key`:  produced character ("a", "Control", "F8").
 *
 * `code` is the right thing to bind to because it's keyboard-layout
 * independent — but its values follow W3C UI Events spec, not rdev's
 * naming. We translate the few that differ.
 */
function browserKeyToRdev(e: KeyboardEvent): string | null {
  // Direct passthroughs (where browser code === rdev name).
  if (PASSTHROUGH.has(e.code)) return e.code;
  // Browser-to-rdev rename map.
  return REMAP[e.code] ?? null;
}

const PASSTHROUGH = new Set([
  // Modifiers + nav (also work as bindable keys)
  'ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight',
  'CapsLock', 'Escape', 'Tab', 'Space', 'Backspace', 'Delete', 'Insert',
  'Home', 'End', 'PageUp', 'PageDown',
  'PrintScreen', 'ScrollLock', 'Pause', 'NumLock',
  // F-keys
  'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12',
  // Letters — browser e.code IS "KeyA".."KeyZ" which matches rdev verbatim.
  'KeyA', 'KeyB', 'KeyC', 'KeyD', 'KeyE', 'KeyF', 'KeyG', 'KeyH', 'KeyI',
  'KeyJ', 'KeyK', 'KeyL', 'KeyM', 'KeyN', 'KeyO', 'KeyP', 'KeyQ', 'KeyR',
  'KeyS', 'KeyT', 'KeyU', 'KeyV', 'KeyW', 'KeyX', 'KeyY', 'KeyZ',
  // Mac/Linux aliases that also happen to match rdev.
  'MetaLeft', 'MetaRight',
  'IntlBackslash',
  'Quote', 'Comma', 'Slash', 'Minus', 'Equal',
]);

// Mapping for browser codes whose names differ from rdev's Debug spelling.
const REMAP: Record<string, string> = {
  // Modifiers
  AltLeft: 'Alt',
  AltRight: 'AltGr',
  OSLeft: 'MetaLeft',  // Linux variant
  OSRight: 'MetaRight',
  // Enter — rdev uses "Return"
  Enter: 'Return',
  NumpadEnter: 'KpReturn',
  // Arrows
  ArrowUp: 'UpArrow',
  ArrowDown: 'DownArrow',
  ArrowLeft: 'LeftArrow',
  ArrowRight: 'RightArrow',
  // Top-row digits — browser "Digit0".."Digit9", rdev "Num0".."Num9"
  Digit0: 'Num0',
  Digit1: 'Num1',
  Digit2: 'Num2',
  Digit3: 'Num3',
  Digit4: 'Num4',
  Digit5: 'Num5',
  Digit6: 'Num6',
  Digit7: 'Num7',
  Digit8: 'Num8',
  Digit9: 'Num9',
  // Symbols — rdev's casing is inconsistent with W3C
  Backquote: 'BackQuote',
  Backslash: 'BackSlash',
  BracketLeft: 'LeftBracket',
  BracketRight: 'RightBracket',
  Semicolon: 'SemiColon',
  Period: 'Dot',
  // Numpad
  Numpad0: 'Kp0',
  Numpad1: 'Kp1',
  Numpad2: 'Kp2',
  Numpad3: 'Kp3',
  Numpad4: 'Kp4',
  Numpad5: 'Kp5',
  Numpad6: 'Kp6',
  Numpad7: 'Kp7',
  Numpad8: 'Kp8',
  Numpad9: 'Kp9',
  NumpadAdd: 'KpPlus',
  NumpadSubtract: 'KpMinus',
  NumpadMultiply: 'KpMultiply',
  NumpadDivide: 'KpDivide',
  NumpadDecimal: 'KpDelete',
};
