// License enforcement switch.
//
// Murmr is currently free, so the license gate is OFF: the app never blocks
// on a missing/invalid key, and the full-screen Paywall is never shown. The
// whole verification mechanism (Ed25519 keys, the `license_status` /
// `set_license_key` commands, the Settings panel, the Paywall component)
// stays wired and ready — flip this single flag to `true` to turn the gate
// on when there's something to sell. No other code change is required.
//
// When true: App renders <Paywall> instead of the main UI until a key with
// LicenseStatus.kind === 'valid' is entered.
export const LICENSE_ENFORCED = false;

// Where the Paywall's "Buy a license" button points. Placeholder until a
// real checkout exists — only surfaced when LICENSE_ENFORCED is true.
export const LICENSE_BUY_URL = 'https://murmr.app/buy';
