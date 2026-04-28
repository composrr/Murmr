#!/usr/bin/env node
// License-key issuer.
//
// Two modes:
//   1. `--init`     Generate a fresh Ed25519 keypair into `.keys/`. Run
//                   ONCE. Bake the printed public key into the next build
//                   via the MURMR_LICENSE_PUBKEY env var.
//   2. (default)    Mint a new license. Reads the private key from
//                   `.keys/license-priv.key`, signs a payload with the
//                   given email + optional expiry + tier, and prints the
//                   complete license string.
//
// Usage:
//   node scripts/issue-license.mjs --init
//   node scripts/issue-license.mjs --email jon@x.com
//   node scripts/issue-license.mjs --email jon@x.com --expires 2027-12-31
//   node scripts/issue-license.mjs --email jon@x.com --tier pro
//
// The generated `.keys/license-priv.key` is ed25519 raw bytes encoded as
// base64url-no-pad. NEVER commit this — `.gitignore` should exclude .keys/.

import {
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign as cryptoSign,
} from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const KEYS_DIR = join(ROOT, '.keys');
const PRIV_PATH = join(KEYS_DIR, 'license-priv.key');
const PUB_PATH = join(KEYS_DIR, 'license-pub.key');

const args = process.argv.slice(2);
function flag(name) {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] ?? null : null;
}
function bool(name) {
  return args.includes(`--${name}`);
}

const isInit = bool('init');
const email = flag('email');
const expiresArg = flag('expires'); // YYYY-MM-DD or full ISO
const tier = flag('tier'); // optional product tier label

function b64urlEncode(buf) {
  return Buffer.from(buf).toString('base64url');
}

function b64urlDecode(s) {
  return Buffer.from(s, 'base64url');
}

function jwkToRawPublicKey(jwk) {
  // JWK 'x' field for OKP/Ed25519 IS the raw 32-byte public key, base64url.
  return b64urlDecode(jwk.x);
}

function jwkToRawPrivateKey(jwk) {
  return b64urlDecode(jwk.d);
}

if (isInit) {
  if (existsSync(PRIV_PATH)) {
    console.error(
      `[issue-license] refusing to overwrite existing key at ${PRIV_PATH}.`,
    );
    console.error(
      `[issue-license] If you really want a new keypair, delete that file first.`,
    );
    process.exit(1);
  }

  console.log('[issue-license] generating Ed25519 keypair…');
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const pubJwk = publicKey.export({ format: 'jwk' });
  const privJwk = privateKey.export({ format: 'jwk' });

  const pubRaw = jwkToRawPublicKey(pubJwk);
  const privRaw = jwkToRawPrivateKey(privJwk);

  const pubB64 = b64urlEncode(pubRaw);
  const privB64 = b64urlEncode(privRaw);

  mkdirSync(KEYS_DIR, { recursive: true });
  writeFileSync(PRIV_PATH, privB64 + '\n', { mode: 0o600 });
  writeFileSync(PUB_PATH, pubB64 + '\n');

  console.log(`[issue-license] wrote private key (${PRIV_PATH})`);
  console.log(`[issue-license] wrote public key  (${PUB_PATH})`);
  console.log('');
  console.log('Bake the public key into your next build:');
  console.log('');
  console.log(`  $env:MURMR_LICENSE_PUBKEY = "${pubB64}"   # PowerShell`);
  console.log(`  export MURMR_LICENSE_PUBKEY="${pubB64}"   # bash/zsh`);
  console.log('');
  console.log(
    'For convenience, scripts/run-tauri.mjs reads this from .keys/license-pub.key',
  );
  console.log('automatically — so this should "just work" on the next npm run tauri build.');
  process.exit(0);
}

// --- Issue a license -------------------------------------------------------

if (!email) {
  console.error('Usage: node scripts/issue-license.mjs --email <addr> [--expires YYYY-MM-DD] [--tier label]');
  console.error('       node scripts/issue-license.mjs --init    # one-time keypair generation');
  process.exit(1);
}

if (!existsSync(PRIV_PATH)) {
  console.error(`[issue-license] no private key at ${PRIV_PATH}.`);
  console.error(`[issue-license] Run with --init first to generate a keypair.`);
  process.exit(1);
}

const privRaw = b64urlDecode(readFileSync(PRIV_PATH, 'utf8').trim());
if (privRaw.length !== 32) {
  console.error(
    `[issue-license] private key is ${privRaw.length} bytes, expected 32.`,
  );
  process.exit(1);
}

// Re-build a Node KeyObject from raw bytes via JWK round-trip.
let pubRaw;
if (existsSync(PUB_PATH)) {
  pubRaw = b64urlDecode(readFileSync(PUB_PATH, 'utf8').trim());
} else {
  // Derive the public key from the private bytes by signing nothing —
  // alternatively we could re-derive via a math op, but Node makes us go
  // through a KeyObject. For now require pub key file.
  console.error(`[issue-license] missing ${PUB_PATH} (run --init to regenerate).`);
  process.exit(1);
}

const privKeyObj = createPrivateKey({
  key: { kty: 'OKP', crv: 'Ed25519', d: b64urlEncode(privRaw), x: b64urlEncode(pubRaw) },
  format: 'jwk',
});

const issuedAt = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
let expiresAtIso = null;
if (expiresArg) {
  // Accept YYYY-MM-DD shorthand or a full ISO string.
  expiresAtIso = expiresArg.includes('T')
    ? expiresArg
    : `${expiresArg}T23:59:59Z`;
}

const payload = {
  email,
  issued_at: issuedAt,
};
if (expiresAtIso) payload.expires_at = expiresAtIso;
if (tier) payload.tier = tier;

const payloadJson = JSON.stringify(payload);
const payloadBytes = Buffer.from(payloadJson, 'utf8');

const sigBytes = cryptoSign(null, payloadBytes, privKeyObj);

const license = `${b64urlEncode(payloadBytes)}.${b64urlEncode(sigBytes)}`;

console.log('');
console.log('License key:');
console.log('');
console.log('  ' + license);
console.log('');
console.log('Payload:', payloadJson);
console.log(`Length:  ${license.length} chars`);
