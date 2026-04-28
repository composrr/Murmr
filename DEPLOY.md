# Deploying Murmr

End-to-end checklist for cutting a release. **First-time setup is in §1.**
After that, shipping a new version is just `git tag vX.Y.Z && git push --tags`.

---

## 1. First-time setup

### 1.1 GitHub repo

**Step A — create the repo on github.com (one click).** Go to
https://github.com/new, name it `murmr`, leave everything else default,
click *Create repository*. Don't add a README/license/.gitignore from the
GitHub UI — we already have those locally.

**Step B — initialize + push from your machine.**

```powershell
cd C:\Users\jondr\Documents\_Claude\Murmr\murmr

git init -b main

# Sanity-check what's about to be committed. Should be ~hundreds of files,
# NOT thousands. If you see node_modules/ or target/ or *.bin in here,
# STOP and check .gitignore.
git status

# Stage + commit. (Reviews .gitignore behaviour — never use `git add -A`
# without checking output first.)
git add .
git commit -m "Initial commit"

# Replace <your-username> below.
git remote add origin https://github.com/<your-username>/murmr.git
git push -u origin main
```

If your GitHub username isn't `composrr`, also fix the updater endpoint in
[`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json):

```json
"endpoints": [
  "http://127.0.0.1:8123/latest.json",
  "https://github.com/<YOUR_USERNAME>/murmr/releases/latest/download/latest.json"
]
```

### 1.2 GitHub secrets — updater signing

The updater will only work if release manifests are signed with the matching
private key. Murmr already generated a key pair at `.tauri/updater.key` (the
public key is committed via `tauri.conf.json`; the private key is gitignored).

Add it to your repo's secrets:

1. **Settings → Secrets and variables → Actions → New repository secret**
2. Name: `TAURI_SIGNING_PRIVATE_KEY`
3. Value: paste the **entire content** of `.tauri/updater.key` (one
   base64-looking line). Get it locally with:
   ```powershell
   Get-Content .tauri\updater.key
   ```
4. Add another secret: `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — leave the
   value empty (we generated the key without a password)

### 1.3 No secret needed for the license public key

The license PUBLIC key (`.keys/license-pub.key`) is committed to the repo —
it's safe by definition (it can only verify, not mint). The wrapper script
auto-loads it at build time, both locally and in CI.

The license PRIVATE key (`.keys/license-priv.key`) stays on your machine
only. NEVER commit it. NEVER paste it into a GitHub secret either —
nothing in CI needs it (CI never mints license keys; you do that locally
with `npm run license:issue --email ...`).

### 1.3 Code signing — what's free, what isn't

| Platform | Result | Cost | Effort |
|---|---|---|---|
| Ship unsigned (current) | SmartScreen / Gatekeeper warning that user clicks past | Free | None — README documents the workaround |
| **Azure Trusted Signing** (Windows) | Real Authenticode signature, no SmartScreen warning | **Free** for individuals as of 2024–25 | ~1 hour: Azure account + ID verification + GitHub Actions hookup |
| Sectigo / DigiCert OV (Windows) | Authenticode, but SmartScreen still warns until reputation builds | $50–200 / year | 30 minutes |
| Sectigo / DigiCert EV (Windows) | Authenticode, no SmartScreen warning ever | $200–400 / year | 30 min + USB hardware token |
| Apple Developer Program (macOS) | Real signing + notarization, no Gatekeeper warning | $99 / year | ~2 hours: enroll + add cert + notarytool to CI |
| Self-sign macOS | Useless — Gatekeeper still blocks | Free | Don't bother |

**Recommended path** for a free release:
- **Windows**: Ship unsigned for the first few releases. README documents the
  one-click "More info → Run anyway" SmartScreen workaround. If you want to
  remove the warning, set up Azure Trusted Signing — it's currently free and
  the closest thing to "real" signing without paying.
- **macOS**: Ship unsigned. README documents the Gatekeeper workaround. Pay
  for the Apple Developer Program once you have users who care.

The **Tauri updater signing** (the key we generated above) is **separate** from
OS code-signing. It only verifies the update manifest itself. Required for the
updater to work at all, free, and we already handle it.

---

## 2. Cutting a release

```bash
cd murmr

# Bump the version everywhere it appears.
# - src-tauri/Cargo.toml         → [package] version = "X.Y.Z"
# - src-tauri/tauri.conf.json    → "version": "X.Y.Z"
# - package.json                 → "version": "X.Y.Z"

git add -A
git commit -m "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main --tags
```

### What happens next

GitHub Actions fires both [`build-windows.yml`](.github/workflows/build-windows.yml)
and [`build-macos.yml`](.github/workflows/build-macos.yml). Within ~10 minutes
each you'll have a draft GitHub Release with:

- `Murmr_X.Y.Z_x64-setup.exe` (Windows NSIS installer)
- `Murmr_X.Y.Z_x64_en-US.msi` (Windows MSI)
- `Murmr_X.Y.Z_aarch64.dmg` (macOS Apple Silicon)
- `Murmr_X.Y.Z_x64.dmg` (macOS Intel)
- `latest.json` — the updater manifest, signed with your private key
- `*.app.tar.gz` archives for the macOS auto-updater

Click **Publish** on the draft release. Existing Murmr installs see the
updater notice within ~24 hours of next launch (or immediately when the user
clicks "Check now" in Settings → General).

---

## 3. Beta loop — push updates to yourself locally

If you're iterating on Murmr and don't want to uninstall/reinstall every time,
the auto-updater can pull from a local HTTP server. No GitHub repo needed.

### One-time setup

You only need this once (after a fresh install of the current version):

1. Install Murmr normally from `src-tauri/target/release/bundle/nsis/`.
2. That's it. The installed app already polls `http://127.0.0.1:8123/latest.json`
   first (configured in `tauri.conf.json`), with GitHub as fallback.

### Per-iteration loop (~4 minutes per cycle)

```powershell
# 1. Make changes in code.
# 2. Build + stage a beta release. Auto-bumps the patch version.
npm run release:beta

# 3. Start the local update server (leave running between iterations).
npm run serve:updater
```

In another terminal — or just the running Murmr — open **Settings → General →
Check for updates**. You'll see a banner: *"vX.Y.Z+1 is available — Install &
restart"*. Click it; the new build downloads from localhost, installs, and
relaunches.

#### What `npm run release:beta` does

- Bumps the patch version in `src-tauri/Cargo.toml`,
  `src-tauri/tauri.conf.json`, and `package.json` (the updater plugin compares
  semver — without a higher version it won't see a new build as an update).
- Runs `npm run tauri build` (which uses the wrapper, gets `/O2` flags, and
  signs the artifacts with `.tauri/updater.key`).
- Reads the generated `.sig` files and writes a `latest.json` manifest into
  `release-staging/`.
- Copies the `.exe` installer into `release-staging/` so the manifest's URL
  resolves.

#### Useful flags

```powershell
# Pin a specific version (e.g. to test downgrade or large jumps).
npm run release:beta -- --version 0.2.0-beta.4

# Re-generate the manifest without rebuilding (after manually tweaking).
npm run release:beta -- --skip-build

# Release notes shown in the updater banner.
npm run release:beta -- --notes "fix: hotkey rebind crash"
```

#### Why localhost-first works for everyone

The Tauri updater tries each endpoint in order. For users not running the
local server, the localhost lookup fails immediately (~1ms — TCP connection
refused, no DNS, no waiting), and it falls through to the GitHub URL. So
shipping with the localhost endpoint baked in costs nothing for normal
users.

#### Why the `dangerousInsecureTransportProtocol: true` flag is safe here

Tauri 2's updater plugin refuses HTTP endpoints by default. We opt in via
`dangerousInsecureTransportProtocol` so the localhost endpoint works.

This is **not** the security risk it sounds like:

- The HTTPS endpoint (GitHub) still does normal certificate validation.
  The flag only allows HTTP URLs to be present in the endpoint list; it
  does NOT downgrade HTTPS to HTTP.
- The actual update payload's integrity is guarded by the **signature
  check** (`.sig` files signed by `.tauri/updater.key`, verified against
  the `pubkey` baked into the binary). Even if someone intercepted the
  HTTP traffic and substituted their own `.exe`, the signature wouldn't
  match and the updater would refuse to install.
- The localhost endpoint binds to `127.0.0.1` only, so it's not reachable
  off the machine. There's no MITM vector.

---

## 4. Local installer build

The `npm run tauri build` wrapper (`scripts/run-tauri.mjs`) handles
everything that's not the same across platforms:

- Discovers MSVC + bundled CMake on Windows, adds them to `PATH`.
- Sets the right `CMAKE_*_FLAGS_RELEASE` / `GGML_AVX*` env vars per
  platform (MSVC needs `/O2 /Ob2 /DNDEBUG`; clang on macOS/Linux already
  has good defaults — the wrapper does NOT inject MSVC flags there because
  clang would interpret `/O2` as a path).
- Auto-loads the updater signing key from `.tauri/updater.key` if present
  (so `.sig` files are produced alongside the installers).

### Windows

Prerequisites:
- Visual Studio 2022 (Community is fine) with the Desktop C++ workload —
  provides MSVC, the Windows SDK, and a bundled CMake.
- LLVM (for `libclang.dll` used by bindgen). Install via
  `winget install LLVM.LLVM`.
- Node 20+ (for the wrapper script and Tauri CLI).

```powershell
# From a fresh PowerShell — the wrapper handles everything.
npm install
npm run tauri build
```

Output:
- `src-tauri\target\release\bundle\msi\Murmr_X.Y.Z_x64_en-US.msi`
- `src-tauri\target\release\bundle\nsis\Murmr_X.Y.Z_x64-setup.exe`

Each comes with a matching `.sig` file (signed by your local
`.tauri/updater.key`) for the auto-updater.

### macOS

Prerequisites:
- macOS 11.0 (Big Sur) or newer.
- Xcode Command Line Tools: `xcode-select --install`. Provides clang,
  Metal SDK headers (needed because we enable Whisper's `metal` feature
  on Apple Silicon for GPU-accelerated inference).
- CMake: `brew install cmake`.
- Node 20+ (`brew install node`).

```bash
git clone <your-fork>
cd murmr
# Drop the Whisper model in the same place the Windows build expects it.
mkdir -p src-tauri/models
curl -L -o src-tauri/models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin

npm install
npm run tauri build
```

Output:
- `src-tauri/target/release/bundle/macos/Murmr.app`
- `src-tauri/target/release/bundle/dmg/Murmr_X.Y.Z_aarch64.dmg`
  (or `_x64.dmg` on Intel Macs)

#### First-run permissions on macOS

On first launch macOS will prompt the user **twice**:

1. **Microphone access** — Murmr asks via the system prompt; user clicks
   *Allow*. Backed by `NSMicrophoneUsageDescription` in
   [`src-tauri/Info.plist`](src-tauri/Info.plist).
2. **Accessibility / Input Monitoring** — when the user first triggers the
   global hotkey, macOS silently denies the keystroke synthesis (enigo)
   and the user has to:
   - Open **System Settings → Privacy & Security → Accessibility**.
   - Toggle **Murmr** on.
   - On macOS 10.15+, also toggle **Murmr** on under **Input Monitoring**.
   - Restart Murmr.

There's no way to programmatically request these — Apple gates them
behind a manual user grant. Document the flow in your release notes if
you're shipping to non-technical users.

#### Code signing on macOS

Unsigned builds open with the Gatekeeper warning *"Murmr can't be opened
because Apple cannot check it for malicious software"*. The user can
right-click → Open to bypass, but it's gross.

To sign + notarize you need an Apple Developer Program membership ($99/yr).
Steps once you have one:

```bash
# Set these in your shell or CI env.
export APPLE_ID="you@apple.id"
export APPLE_PASSWORD="app-specific-password"  # NOT your Apple ID password
export APPLE_TEAM_ID="ABCD1234"
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (ABCD1234)"

npm run tauri build
```

Tauri then signs with hardened runtime + the entitlements in
[`src-tauri/entitlements.plist`](src-tauri/entitlements.plist) and
submits to Apple's notary service. The DMG comes back with a stamp;
Gatekeeper passes silently.

### Linux

Not yet tested. The Rust code uses `cfg(unix)` for the obvious bits
(injector uses Ctrl, focus module degrades to None) but no `.deb` /
`.AppImage` smoke-tests have been done.

---

## 5. Versioning convention

We use [SemVer](https://semver.org). Roughly:

- `0.x.y` — pre-1.0, breaking changes allowed in minor bumps
- `1.x.y` — first stable, breaking changes only on major bumps
- `x.y.0` → `x.y.1` — bug fixes only
- `x.y.0` → `x.y+1.0` — backwards-compatible features
- `x.0.0` → `x+1.0.0` — breaking changes

If a release breaks the SQLite schema, settings format, or hotkey hook in a
way that requires user action, mention it in the GitHub Release notes
prominently. The auto-updater doesn't ask for confirmation, so users will
land on the new version immediately.
