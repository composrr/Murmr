#!/usr/bin/env node
// Wrapper around `tauri` that ensures CMake (needed by whisper-rs's whisper.cpp build)
// is on PATH. On Windows we fall back to the CMake bundled with Visual Studio Build Tools
// when no standalone cmake.exe is present. Cross-platform: on macOS/Linux we just exec.

import { execSync, spawn } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { platform, arch, env } from 'node:process';

function which(cmd) {
  try {
    const probe = platform === 'win32' ? `where ${cmd}` : `which ${cmd}`;
    const out = execSync(probe, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
    return out.split(/\r?\n/)[0] || null;
  } catch {
    return null;
  }
}

function findVSBundledCMake() {
  // Visual Studio bundles CMake at:
  //   <VS root>\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe
  // VS root looks like: C:\Program Files (x86)\Microsoft Visual Studio\<year-or-major>\<edition>
  const roots = [
    'C:\\Program Files\\Microsoft Visual Studio',
    'C:\\Program Files (x86)\\Microsoft Visual Studio',
  ];
  for (const root of roots) {
    if (!existsSync(root)) continue;
    let majors;
    try {
      majors = readdirSync(root).filter((d) => statSync(join(root, d)).isDirectory());
    } catch {
      continue;
    }
    for (const major of majors) {
      const editionsDir = join(root, major);
      let editions;
      try {
        editions = readdirSync(editionsDir).filter((d) =>
          statSync(join(editionsDir, d)).isDirectory(),
        );
      } catch {
        continue;
      }
      for (const edition of editions) {
        const cmakeBin = join(
          editionsDir,
          edition,
          'Common7',
          'IDE',
          'CommonExtensions',
          'Microsoft',
          'CMake',
          'CMake',
          'bin',
        );
        if (existsSync(join(cmakeBin, 'cmake.exe'))) return cmakeBin;
      }
    }
  }
  return null;
}

function findVSMSVC() {
  // Returns paths to add for cl.exe + linker.
  const roots = [
    'C:\\Program Files\\Microsoft Visual Studio',
    'C:\\Program Files (x86)\\Microsoft Visual Studio',
  ];
  for (const root of roots) {
    if (!existsSync(root)) continue;
    let majors;
    try {
      majors = readdirSync(root).filter((d) => statSync(join(root, d)).isDirectory());
    } catch {
      continue;
    }
    for (const major of majors) {
      for (const edition of readdirSync(join(root, major))) {
        const msvcDir = join(root, major, edition, 'VC', 'Tools', 'MSVC');
        if (!existsSync(msvcDir)) continue;
        const versions = readdirSync(msvcDir)
          .filter((d) => statSync(join(msvcDir, d)).isDirectory())
          .sort()
          .reverse();
        if (!versions.length) continue;
        const ver = versions[0];
        const hostBin = join(msvcDir, ver, 'bin', 'Hostx64', 'x64');
        if (existsSync(join(hostBin, 'cl.exe'))) return hostBin;
      }
    }
  }
  return null;
}

function findLibClang() {
  const candidates = [
    'C:\\Program Files\\LLVM\\bin',
    'C:\\Program Files (x86)\\LLVM\\bin',
  ];
  for (const dir of candidates) {
    if (existsSync(join(dir, 'libclang.dll'))) return dir;
  }
  return null;
}

const newPath = [];
const extraEnv = {};

// ---------------------------------------------------------------------------
// whisper.cpp build-time env vars (forwarded to CMake by whisper-rs-sys's
// build.rs which passes through anything starting with CMAKE_ / WHISPER_ /
// GGML_).
//
// These MUST be set platform-conditionally because cmake-rs's MSVC defaults
// omit /O2 (causing whisper.cpp to compile at /Od → 10× slower at runtime),
// while clang on macOS/Linux already gets sensible -O3 defaults from CMake
// and would choke on `/O2` (it'd interpret the leading `/` as a path).
//
// We layer the env vars in here rather than in `.cargo/config.toml` because
// cargo's `[env]` table doesn't support per-target conditionals.
// ---------------------------------------------------------------------------
function setWhisperBuildEnv() {
  if (platform === 'win32') {
    // MSVC with cmake-rs leaves Release flags at ` -nologo -MD -Brepro -W0`
    // (no /O2). Force the canonical Release optimisation set so whisper.cpp's
    // GEMM kernels don't run at /Od.
    extraEnv.CMAKE_C_FLAGS_RELEASE = '/O2 /Ob2 /DNDEBUG /MD -nologo -Brepro -W0';
    extraEnv.CMAKE_CXX_FLAGS_RELEASE = '/O2 /Ob2 /DNDEBUG /MD /utf-8 -nologo -Brepro -W0';
    // ggml's GGML_NATIVE switch is a no-op on MSVC (it only emits
    // -march=native on GCC/Clang). Without these, the compiler doesn't emit
    // AVX2/FMA SIMD instructions even though the CPU supports them.
    extraEnv.GGML_AVX = 'ON';
    extraEnv.GGML_AVX2 = 'ON';
    extraEnv.GGML_FMA = 'ON';
    extraEnv.GGML_F16C = 'ON';
  }
  // macOS + Linux: cmake's defaults include -O3 already. clang/gcc handle
  // SIMD via -march=native (which GGML_NATIVE=ON already turns on).
  // No overrides needed — and importantly, we MUST NOT set MSVC-style
  // `/O2` flags here because clang would interpret them as paths.
}

setWhisperBuildEnv();

if (platform === 'win32') {
  if (!which('cmake')) {
    const vsCMake = findVSBundledCMake();
    if (vsCMake) {
      newPath.push(vsCMake);
      console.log(`[run-tauri] adding bundled CMake to PATH: ${vsCMake}`);
    } else {
      console.warn(
        '[run-tauri] cmake.exe not found and no VS-bundled CMake located. whisper-rs may fail to build.',
      );
    }
  }
  if (!which('cl.exe') && !which('cl')) {
    const msvc = findVSMSVC();
    if (msvc) {
      newPath.push(msvc);
      console.log(`[run-tauri] adding MSVC bin to PATH: ${msvc}`);
    }
  }
  // bindgen (used by whisper-rs-sys) needs libclang.dll. Prefer an existing
  // LIBCLANG_PATH; otherwise discover the standard LLVM install.
  if (!env.LIBCLANG_PATH) {
    const libclang = findLibClang();
    if (libclang) {
      extraEnv.LIBCLANG_PATH = libclang;
      newPath.push(libclang);
      console.log(`[run-tauri] setting LIBCLANG_PATH=${libclang}`);
    } else {
      console.warn(
        '[run-tauri] libclang.dll not found. Install LLVM (e.g. `winget install LLVM.LLVM`) so whisper-rs-sys can build.',
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Updater signing key — Tauri reads TAURI_SIGNING_PRIVATE_KEY from the env
// when bundling, and emits matching `.sig` files alongside each installer.
// We keep the unencrypted key at .tauri/updater.key (gitignored) so a `tauri
// build` "just works" without the user having to copy/paste secrets.
// CI sets the env var directly from a GitHub secret; this fallback is for
// local builds.
// ---------------------------------------------------------------------------
function loadLocalSigningKey() {
  if (env.TAURI_SIGNING_PRIVATE_KEY) return; // already set (CI / shell export)
  const candidates = [
    join('.tauri', 'updater.key'),
    join(homedir(), '.tauri', 'murmr-updater.key'),
  ];
  for (const path of candidates) {
    if (!existsSync(path)) continue;
    try {
      const key = readFileSync(path, 'utf8').trim();
      if (key) {
        extraEnv.TAURI_SIGNING_PRIVATE_KEY = key;
        // Empty-string password matches how the key was generated:
        //   `tauri signer generate -w .tauri/updater.key -p ""`
        if (!env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
          extraEnv.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '';
        }
        console.log(`[run-tauri] loaded updater signing key from ${path}`);
        return;
      }
    } catch (e) {
      console.warn(`[run-tauri] failed to read signing key at ${path}: ${e.message}`);
    }
  }
}

loadLocalSigningKey();

// ---------------------------------------------------------------------------
// Apple code-signing + notarization creds for LOCAL macOS builds.
//
// tauri-bundler reads these from the env to sign (Developer ID) and notarize:
//   APPLE_SIGNING_IDENTITY  "Developer ID Application: Name (TEAMID)"
//   APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID   (notarization; APPLE_PASSWORD
//                                                is an app-specific password)
// Optionally APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD to import a .p12
// (not needed locally when the identity already lives in the login keychain).
//
// We load them from a gitignored `.tauri/apple.env` (KEY=VALUE per line) so a
// local `npm run tauri build` "just works". CI sets these from GitHub Secrets
// and never reads this file. Anything already in the environment wins.
// ---------------------------------------------------------------------------
function loadLocalAppleCredentials() {
  if (platform !== 'darwin') return; // Apple signing only applies on macOS
  const path = join('.tauri', 'apple.env');
  if (!existsSync(path)) return;
  let loaded = 0;
  try {
    for (const raw of readFileSync(path, 'utf8').split(/\r?\n/)) {
      const line = raw.trim();
      if (!line || line.startsWith('#')) continue;
      const eq = line.indexOf('=');
      if (eq === -1) continue;
      const key = line.slice(0, eq).trim();
      let val = line.slice(eq + 1).trim();
      // Strip a single pair of surrounding quotes if present.
      if (val.length >= 2 && ((val[0] === '"' && val.at(-1) === '"') ||
                              (val[0] === "'" && val.at(-1) === "'"))) {
        val = val.slice(1, -1);
      }
      if (!key || key in env || key in extraEnv) continue; // env wins
      extraEnv[key] = val;
      loaded++;
    }
    if (loaded) console.log(`[run-tauri] loaded ${loaded} Apple credential var(s) from ${path}`);
  } catch (e) {
    console.warn(`[run-tauri] failed to read Apple credentials at ${path}: ${e.message}`);
  }
}

loadLocalAppleCredentials();

// ---------------------------------------------------------------------------
// License-validation public key — baked into the binary via the
// MURMR_LICENSE_PUBKEY build env var (read by option_env! in
// src-tauri/src/license/mod.rs). Generated once with
// `node scripts/issue-license.mjs --init`, which writes the matching private
// key to .keys/license-priv.key (gitignored). The public key at
// .keys/license-pub.key IS committed, so both local and CI builds bake it in
// automatically — no secret needed. If it's absent the binary still builds,
// but every license key is rejected (which is fine while enforcement is off).
// ---------------------------------------------------------------------------
function loadLocalLicensePubkey() {
  if (env.MURMR_LICENSE_PUBKEY) return; // explicit override (CI / shell)
  const path = join('.keys', 'license-pub.key');
  if (!existsSync(path)) {
    console.warn(
      '[run-tauri] no .keys/license-pub.key — license validation will reject all keys.',
    );
    console.warn(
      '[run-tauri] Run `node scripts/issue-license.mjs --init` to generate one.',
    );
    return;
  }
  try {
    const key = readFileSync(path, 'utf8').trim();
    if (key) {
      extraEnv.MURMR_LICENSE_PUBKEY = key;
      console.log(`[run-tauri] baked license pubkey from ${path}`);
    }
  } catch (e) {
    console.warn(`[run-tauri] failed to read license pubkey: ${e.message}`);
  }
}

loadLocalLicensePubkey();

const PATH = [...newPath, env.PATH].join(platform === 'win32' ? ';' : ':');

const args = process.argv.slice(2);
const tauriCli = join('node_modules', '@tauri-apps', 'cli', 'tauri.js');

const child = spawn(process.execPath, [tauriCli, ...args], {
  stdio: 'inherit',
  env: { ...env, ...extraEnv, PATH },
});

child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 0);
});
