#!/usr/bin/env node
// Tiny static file server for the Tauri auto-updater beta loop.
//
// Serves `release-staging/` on http://127.0.0.1:8123. The Tauri updater
// (configured in tauri.conf.json) hits this URL first, falling back to
// GitHub Releases when localhost isn't reachable.
//
// Why hand-rolled instead of `npx serve`? No npm install needed (this runs
// straight from `node scripts/serve-updater.mjs`), and we get tight control
// over CORS + Range support so the updater plugin's HTTP client doesn't
// trip on any quirk.

import { createServer } from 'node:http';
import { createReadStream, statSync, existsSync } from 'node:fs';
import { dirname, join, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const STAGING = resolve(__dirname, '..', 'release-staging');
const PORT = 8123;
const HOST = '127.0.0.1'; // localhost-only — never bind 0.0.0.0 here

if (!existsSync(STAGING)) {
  console.error(`[serve-updater] no release-staging/ folder. Run "npm run release:beta" first.`);
  process.exit(1);
}

const MIME = {
  '.json': 'application/json',
  '.exe': 'application/octet-stream',
  '.msi': 'application/octet-stream',
  '.dmg': 'application/octet-stream',
  '.gz':  'application/gzip',
  '.sig': 'text/plain',
};

const server = createServer((req, res) => {
  const url = new URL(req.url ?? '/', `http://${HOST}:${PORT}`);
  // Strip leading / and any path traversal attempts.
  const safeName = basename(decodeURIComponent(url.pathname));
  const filePath = safeName ? join(STAGING, safeName) : '';

  // Always log so the user can see when the app polls.
  const logPrefix = `[serve-updater] ${new Date().toISOString().slice(11, 19)} ${req.method} ${req.url}`;

  if (!filePath || !existsSync(filePath)) {
    console.log(`${logPrefix} → 404`);
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('not found');
    return;
  }

  const stat = statSync(filePath);
  if (!stat.isFile()) {
    console.log(`${logPrefix} → 404 (not file)`);
    res.writeHead(404).end('not found');
    return;
  }

  const ext = ('.' + safeName.split('.').pop()).toLowerCase();
  const headers = {
    'Content-Type': MIME[ext] ?? 'application/octet-stream',
    'Content-Length': String(stat.size),
    'Cache-Control': 'no-cache, no-store, must-revalidate',
    // The updater downloads big binaries — let the http client know we
    // accept range requests in case it wants to resume.
    'Accept-Ranges': 'bytes',
  };

  // Minimal Range support — the updater plugin doesn't use it today but
  // if it ever does we don't want to silently send a full body that the
  // client truncates.
  const range = req.headers.range;
  if (range) {
    const match = /bytes=(\d+)-(\d*)/.exec(range);
    if (match) {
      const start = parseInt(match[1], 10);
      const end = match[2] ? parseInt(match[2], 10) : stat.size - 1;
      if (start < stat.size && end < stat.size && start <= end) {
        headers['Content-Length'] = String(end - start + 1);
        headers['Content-Range'] = `bytes ${start}-${end}/${stat.size}`;
        console.log(`${logPrefix} → 206 (${start}-${end})`);
        res.writeHead(206, headers);
        createReadStream(filePath, { start, end }).pipe(res);
        return;
      }
    }
  }

  console.log(`${logPrefix} → 200 (${(stat.size / 1024 / 1024).toFixed(1)} MB)`);
  res.writeHead(200, headers);
  createReadStream(filePath).pipe(res);
});

server.listen(PORT, HOST, () => {
  console.log(`[serve-updater] serving ${STAGING}`);
  console.log(`[serve-updater] listening on http://${HOST}:${PORT}/`);
  console.log(`[serve-updater] manifest URL:  http://${HOST}:${PORT}/latest.json`);
  console.log(`[serve-updater] press Ctrl+C to stop`);
});

process.on('SIGINT', () => {
  server.close(() => process.exit(0));
});
