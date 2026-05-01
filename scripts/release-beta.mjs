#!/usr/bin/env node
// Build a beta release locally and stage it for the auto-updater.
//
// Workflow:
//   1. Bump the patch version (or use --version) across the three places
//      Tauri reads it from: src-tauri/Cargo.toml, src-tauri/tauri.conf.json,
//      package.json. Tauri's updater plugin compares semver, so each beta
//      build NEEDS a higher version than the installed one or the running
//      app won't see it as an update.
//   2. Run `npm run tauri build` (which invokes the wrapper, which sets
//      the right MSVC /O2 flags + auto-loads the signing key).
//   3. Read the .sig files Tauri produced for the NSIS installer (Win) /
//      .app.tar.gz (mac) and base64-encode them into a `latest.json`
//      manifest matching the updater plugin's expected schema.
//   4. Copy the installer + manifest into `release-staging/`. The
//      `serve-updater` script serves that folder over localhost:8123.
//
// Usage:
//   node scripts/release-beta.mjs                  # auto-bump patch
//   node scripts/release-beta.mjs --version 0.1.5  # explicit version
//   node scripts/release-beta.mjs --notes "fix: ..."
//   node scripts/release-beta.mjs --skip-build     # rebuild manifest only

import { execSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { platform, arch } from 'node:process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const STAGING = join(ROOT, 'release-staging');

// --- Args ------------------------------------------------------------------

const args = process.argv.slice(2);
function flag(name) {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] ?? true : null;
}
function bool(name) {
  return args.includes(`--${name}`);
}

const explicitVersion = flag('version');
const releaseNotes = flag('notes') || 'Beta build — local testing.';
const skipBuild = bool('skip-build');

// --- Step 1: bump version --------------------------------------------------

const tauriConfPath = join(ROOT, 'src-tauri', 'tauri.conf.json');
const cargoTomlPath = join(ROOT, 'src-tauri', 'Cargo.toml');
const packageJsonPath = join(ROOT, 'package.json');

const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf8'));
const oldVersion = tauriConf.version;

const newVersion = explicitVersion || bumpPatch(oldVersion);
console.log(`[release-beta] bumping version: ${oldVersion} → ${newVersion}`);

// Sanity check: if there are uncommitted SOURCE changes (anything other
// than version files we're about to bump), refuse to release. The
// auto-commit step below only stages version files — without this check,
// it's painfully easy to release CI builds that don't include the source
// changes you JUST made (this exact bug ate v0.1.24, v0.1.25, v0.1.26).
{
  const dirty = execSync('git status --porcelain', { cwd: ROOT })
    .toString()
    .split('\n')
    .filter(Boolean)
    .map((l) => l.slice(3)) // strip 'XX ' prefix
    .filter((path) => {
      // The version-bump files ARE going to be committed by us. Ignore.
      const versionFiles = new Set([
        'src-tauri/Cargo.toml',
        'src-tauri/Cargo.lock',
        'src-tauri/tauri.conf.json',
        'package.json',
      ]);
      return !versionFiles.has(path);
    });
  if (dirty.length > 0) {
    console.error('[release-beta] refusing to release: uncommitted changes in:');
    for (const path of dirty) console.error('  ' + path);
    console.error('');
    console.error('  Run `git add` + `git commit` before release-beta, or');
    console.error('  pass --allow-dirty if you really want to release without them.');
    if (!args.includes('--allow-dirty')) process.exit(1);
  }
}

if (newVersion !== oldVersion) {
  // tauri.conf.json
  tauriConf.version = newVersion;
  writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');

  // Cargo.toml — replace `version = "X.Y.Z"` in [package] section.
  let cargoToml = readFileSync(cargoTomlPath, 'utf8');
  cargoToml = cargoToml.replace(
    /(\[package\][^\[]*?\nversion\s*=\s*)"[^"]+"/,
    `$1"${newVersion}"`,
  );
  writeFileSync(cargoTomlPath, cargoToml);

  // package.json
  const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
  pkg.version = newVersion;
  writeFileSync(packageJsonPath, JSON.stringify(pkg, null, 2) + '\n');

  // Auto-commit + push the version bump. Critical: without this, a later
  // `git tag vX.Y.Z` lands on a commit where source still says the OLD
  // version → CI builds installers named with the old version → OTA
  // delivers a binary that immediately re-detects an update → infinite
  // loop. Lesson learned the hard way at v0.1.20.
  try {
    execSync(
      `git add src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock package.json`,
      { cwd: ROOT, stdio: 'pipe' },
    );
    // Empty diff (e.g. user already committed) → nothing to commit, skip.
    const status = execSync('git diff --cached --name-only', {
      cwd: ROOT,
    }).toString().trim();
    if (status) {
      execSync(`git commit -m "Bump version to ${newVersion}"`, {
        cwd: ROOT,
        stdio: 'pipe',
      });
      // Don't auto-push — let the user push when they're ready (avoids
      // surprise commits on `main` mid-development). The commit IS made
      // so a subsequent `git tag` lands on the right SHA.
      console.log(
        `[release-beta] committed version bump (NOT pushed — run \`git push\` before tagging)`,
      );
    }
  } catch (e) {
    console.warn(
      `[release-beta] couldn't auto-commit version bump: ${e.message}\n` +
        `[release-beta] make sure to commit ${newVersion} before tagging!`,
    );
  }
}

// --- Step 2: build ---------------------------------------------------------

if (!skipBuild) {
  console.log('[release-beta] running npm run tauri build (this takes 3-5 min)');
  execSync('npm run tauri build', {
    cwd: ROOT,
    stdio: 'inherit',
  });
} else {
  console.log('[release-beta] --skip-build set, regenerating manifest from existing artifacts');
}

// --- Step 3: locate artifacts + read .sig ---------------------------------

// Respect CARGO_TARGET_DIR — useful when Defender locks the normal target
// dir and we need to build to a sibling location instead.
const cargoTargetDir = process.env.CARGO_TARGET_DIR || join(ROOT, 'src-tauri', 'target');
const targetDir = join(cargoTargetDir, 'release', 'bundle');

// Tauri's NSIS bundler emits the installer + a separate signature file. We
// upload the installer as-is and fold the .sig contents into latest.json.
//
// Filenames embed the version (`Murmr_0.1.5_x64-setup.exe`). After several
// builds the bundle dir holds installers from prior versions too, so we MUST
// filter by `newVersion` — naively picking the first match by suffix gives
// you whichever sorts alphabetically first, which is almost never what you
// want.
function findArtifact(subdir, suffix) {
  const dir = join(targetDir, subdir);
  if (!existsSync(dir)) return null;
  const files = readdirSync(dir);
  // Prefer an exact version match; fall back to any matching suffix only if
  // nothing matches (paranoid fallback — should never fire in practice).
  const versioned = files.find(
    (f) => f.includes(`_${newVersion}_`) && f.endsWith(suffix),
  );
  if (versioned) return versioned;
  return files.find((f) => f.endsWith(suffix)) ?? null;
}

const platforms = {};

if (platform === 'win32') {
  const exeFile = findArtifact('nsis', '-setup.exe');
  const sigFile = findArtifact('nsis', '-setup.exe.sig');
  if (!exeFile || !sigFile) {
    console.error('[release-beta] could not find NSIS installer or .sig under', join(targetDir, 'nsis'));
    process.exit(1);
  }
  const sig = readFileSync(join(targetDir, 'nsis', sigFile), 'utf8').trim();
  // Tauri serves NSIS installers under platform key `windows-x86_64`. The
  // architecture suffix matches what the updater plugin's `target_triple()`
  // returns, NOT Node's `arch` value.
  platforms['windows-x86_64'] = {
    signature: sig,
    url: `http://127.0.0.1:8123/${exeFile}`,
  };
}

if (platform === 'darwin') {
  // Tauri produces an .app.tar.gz for the macOS updater (NOT the .dmg —
  // the .dmg is for first installs only). The .sig sits next to it.
  const archDir = arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
  const macosDir = join(ROOT, 'src-tauri', 'target', archDir, 'release', 'bundle', 'macos');
  const fallbackMacosDir = join(targetDir, 'macos');
  const dir = existsSync(macosDir) ? macosDir : fallbackMacosDir;

  if (!existsSync(dir)) {
    console.error('[release-beta] no macos bundle dir at', dir);
    process.exit(1);
  }
  const files = readdirSync(dir);
  const pickVersioned = (suffix) =>
    files.find((f) => f.includes(`_${newVersion}_`) && f.endsWith(suffix)) ??
    files.find((f) => f.endsWith(suffix));
  const tarball = pickVersioned('.app.tar.gz');
  const sigFile = pickVersioned('.app.tar.gz.sig');
  if (!tarball || !sigFile) {
    console.error('[release-beta] missing .app.tar.gz or .sig in', dir);
    process.exit(1);
  }
  const sig = readFileSync(join(dir, sigFile), 'utf8').trim();
  const platformKey = arch === 'arm64' ? 'darwin-aarch64' : 'darwin-x86_64';
  platforms[platformKey] = {
    signature: sig,
    url: `http://127.0.0.1:8123/${tarball}`,
  };
}

if (Object.keys(platforms).length === 0) {
  console.error('[release-beta] no platforms produced — aborting');
  process.exit(1);
}

// --- Step 4: write manifest + copy artifacts ------------------------------

mkdirSync(STAGING, { recursive: true });

const manifest = {
  version: newVersion,
  notes: releaseNotes,
  pub_date: new Date().toISOString(),
  platforms,
};
writeFileSync(join(STAGING, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');

// Copy the actual installer payload(s) so the URL in the manifest resolves.
for (const [_platformKey, p] of Object.entries(platforms)) {
  const filename = p.url.split('/').pop();
  const sourceDir = platform === 'win32' ? join(targetDir, 'nsis') : pickMacosDir(targetDir);
  const src = join(sourceDir, filename);
  const dst = join(STAGING, filename);
  copyFileSync(src, dst);
  console.log(`[release-beta] staged ${filename} (${(statSync(dst).size / 1024 / 1024).toFixed(1)} MB)`);
}

console.log(`\n[release-beta] ✓ done`);
console.log(`  manifest: ${join(STAGING, 'latest.json')}`);
console.log(`  next:     npm run serve:updater   (then click "Check for updates" in Murmr)`);

// --- helpers ---------------------------------------------------------------

function bumpPatch(v) {
  // Strip any pre-release suffix and bump patch. e.g. "0.1.0" → "0.1.1",
  // "0.1.0-beta.3" → "0.1.1".
  const [core] = v.split('-');
  const parts = core.split('.').map((n) => parseInt(n, 10));
  if (parts.length !== 3 || parts.some(Number.isNaN)) {
    throw new Error(`unexpected version format: ${v}`);
  }
  parts[2] += 1;
  return parts.join('.');
}

function pickMacosDir(baseTargetDir) {
  // For mac, bundles can live in target/release/bundle/macos OR
  // target/<arch>-apple-darwin/release/bundle/macos depending on whether
  // --target was passed. Try both.
  const candidates = [
    join(baseTargetDir, 'macos'),
    join(ROOT, 'src-tauri', 'target', 'aarch64-apple-darwin', 'release', 'bundle', 'macos'),
    join(ROOT, 'src-tauri', 'target', 'x86_64-apple-darwin', 'release', 'bundle', 'macos'),
  ];
  return candidates.find((c) => existsSync(c)) ?? candidates[0];
}
