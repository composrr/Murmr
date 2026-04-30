import { useEffect, useState, type ReactNode } from 'react';

/**
 * Modal that renders Murmr's CHANGELOG.md, fetched from the GitHub raw URL.
 *
 * One source of truth: the .md file in the repo. This component renders it
 * with brand styling (Source Serif headings, tag pills for New/Improved/
 * Fixed). Same parser shape as `tools/changelog-page.html` — when we move
 * to a marketing site, both surfaces stay in sync via the .md.
 *
 * Cache: response is stashed in localStorage with a 1h TTL so opening this
 * modal twice doesn't double-fetch. Stale cache > network call when GitHub's
 * having a moment.
 */

const CHANGELOG_URL =
  'https://raw.githubusercontent.com/composrr/Murmr/main/CHANGELOG.md';
const CACHE_KEY = 'murmr.changelogMd';
const CACHE_TTL_MS = 60 * 60 * 1000; // 1h

export default function ReleaseNotes({ onClose }: { onClose: () => void }) {
  const [md, setMd] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Try cache first.
    try {
      const cached = localStorage.getItem(CACHE_KEY);
      if (cached) {
        const { ts, body } = JSON.parse(cached) as { ts: number; body: string };
        if (Date.now() - ts < CACHE_TTL_MS && body) {
          setMd(body);
          return;
        }
      }
    } catch {}

    fetch(CHANGELOG_URL, { cache: 'no-cache' })
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.text();
      })
      .then((body) => {
        setMd(body);
        try {
          localStorage.setItem(CACHE_KEY, JSON.stringify({ ts: Date.now(), body }));
        } catch {}
      })
      .catch((e) => setError(String(e?.message ?? e)));
  }, []);

  // Esc key closes the modal.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm flex items-center justify-center p-6"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-[680px] max-h-[80vh] bg-bg-content border border-border-hairline rounded-[16px] shadow-2xl flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 py-4 border-b border-border-hairline flex-shrink-0">
          <h2 className="font-serif text-[22px] tracking-[-0.3px] text-text-primary m-0">
            What's new
          </h2>
          <button
            onClick={onClose}
            className="text-text-tertiary hover:text-text-primary text-[13px] px-2 py-1 rounded"
            title="Close (Esc)"
          >
            ✕
          </button>
        </div>

        <div className="overflow-y-auto px-6 py-5 text-[14px] leading-[1.55]">
          {md === null && error === null && (
            <div className="text-text-tertiary text-center py-8 text-[13px]">
              Loading release notes…
            </div>
          )}
          {error && (
            <div className="text-[#c14a2b] dark:text-[#e87a5e] text-[13px] py-3">
              Couldn't load CHANGELOG.md ({error}). Try again later — or view on{' '}
              <a
                href="https://github.com/composrr/Murmr/releases"
                target="_blank"
                rel="noreferrer"
                className="underline"
              >
                GitHub Releases
              </a>
              .
            </div>
          )}
          {md && <ChangelogBody md={md} />}
        </div>

        <div className="px-6 py-3 border-t border-border-hairline flex justify-between items-center flex-shrink-0 text-[12px] text-text-quaternary">
          <span>Source: CHANGELOG.md</span>
          <a
            href="https://github.com/composrr/Murmr/releases"
            target="_blank"
            rel="noreferrer"
            className="text-text-tertiary hover:text-text-primary"
          >
            View on GitHub →
          </a>
        </div>
      </div>
    </div>
  );
}

/**
 * Tiny markdown renderer for the limited grammar CHANGELOG.md uses
 * (`## release`, `### subsection`, list items, code, bold, links).
 * Mirrors the parser in `tools/changelog-page.html` — same input ⇒ same
 * structure on both surfaces.
 */
function ChangelogBody({ md }: { md: string }) {
  // Drop everything before the first `## ` so we skip the prose header.
  const idx = md.indexOf('\n## ');
  const body = idx >= 0 ? md.slice(idx + 1) : md;
  const releases = body.split(/\n(?=## )/);

  return (
    <div>
      {releases.map((chunk, i) => (
        <ReleaseSection key={i} chunk={chunk} />
      ))}
    </div>
  );
}

function ReleaseSection({ chunk }: { chunk: string }) {
  const lines = chunk.split('\n');
  const head = lines.shift() || '';
  if (!head.startsWith('## ')) return null;

  const headRest = head.slice(3).trim();
  const isUnreleased = /^\[?Unreleased\]?$/i.test(headRest);
  let title = headRest;
  let date = '';
  let tagline = '';
  if (!isUnreleased) {
    const parts = headRest.split('—').map((s) => s.trim());
    title = parts[0] || headRest;
    date = parts[1] || '';
    tagline = parts.slice(2).join(' — ');
  } else {
    title = 'Unreleased';
  }

  // Group lines into [{ heading, items }] sections.
  type Section = { heading: string | null; items: string[] };
  const sections: Section[] = [];
  let current: Section | null = null;
  let prosePara: string | null = null;

  function flushPara(into: Section | null) {
    // unused; keep for symmetry
    void into;
  }

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    if (trimmed.startsWith('_') && trimmed.endsWith('_')) {
      // Italic prose paragraph (e.g. the "Anything currently in main" hint
      // for Unreleased)
      prosePara = trimmed.slice(1, -1);
      continue;
    }

    if (trimmed.startsWith('### ')) {
      flushPara(current);
      current = { heading: trimmed.slice(4).trim(), items: [] };
      sections.push(current);
      continue;
    }

    if (trimmed.startsWith('- ')) {
      if (!current) {
        current = { heading: null, items: [] };
        sections.push(current);
      }
      current.items.push(trimmed.slice(2));
      continue;
    }

    // Continuation of previous list item
    if (current && current.items.length && /^\s+\S/.test(line)) {
      current.items[current.items.length - 1] += ' ' + trimmed;
    }
  }

  return (
    <section
      className={[
        'mb-5 rounded-[12px] border p-5',
        isUnreleased
          ? 'border-dashed border-border-hairline bg-transparent'
          : 'border-border-hairline bg-bg-row',
      ].join(' ')}
    >
      <div className="flex items-baseline justify-between gap-3 mb-1">
        <h3 className="font-serif text-[18px] tracking-[-0.2px] text-text-primary m-0">
          {title}
        </h3>
        {date && (
          <span className="text-text-tertiary text-[12px] tabular-nums">{date}</span>
        )}
      </div>
      {tagline && (
        <p className="text-text-tertiary text-[12px] mb-3 mt-0">{tagline}</p>
      )}
      {prosePara && (
        <p className="text-text-tertiary italic text-[12.5px] my-2">
          <Inline text={prosePara} />
        </p>
      )}
      {sections.map((s, i) => (
        <div key={i}>
          {s.heading && (
            <span
              className={[
                'inline-block uppercase tracking-[0.08em] text-[10.5px] font-semibold px-2 py-[2px] rounded-full mt-3 mb-1.5',
                tagColor(s.heading),
              ].join(' ')}
            >
              {s.heading}
            </span>
          )}
          <ul className="list-none pl-0 m-0">
            {s.items.map((item, j) => (
              <li
                key={j}
                className="relative pl-4 py-[3px] text-[13.5px] text-text-primary before:absolute before:left-0 before:top-[12px] before:w-[4px] before:h-[4px] before:rounded-full before:bg-text-quaternary"
              >
                <Inline text={item} />
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}

function tagColor(heading: string): string {
  switch (heading.toLowerCase()) {
    case 'new':
      return 'bg-[#e8f1e6] text-[#4a7f3f] dark:bg-[#28341f] dark:text-[#93c98a]';
    case 'improved':
      return 'bg-[#e6eef5] text-[#1f5c8c] dark:bg-[#1c2c3b] dark:text-[#7eb1da]';
    case 'fixed':
      return 'bg-[#fbe9e3] text-[#c14a2b] dark:bg-[#3a1f15] dark:text-[#e87a5e]';
    default:
      return 'bg-bg-control text-text-secondary';
  }
}

/** Render inline markdown: **bold**, `code`, [text](url). */
function Inline({ text }: { text: string }) {
  // Walk the string and emit React nodes. Order: code → bold → links.
  const nodes: ReactNode[] = [];
  let rest = text;
  let key = 0;

  while (rest.length > 0) {
    // `code`
    const codeMatch = /`([^`]+)`/.exec(rest);
    // **bold**
    const boldMatch = /\*\*(.+?)\*\*/.exec(rest);
    // [link](url)
    const linkMatch = /\[([^\]]+)\]\(([^)]+)\)/.exec(rest);

    // Find earliest match
    const candidates = [
      codeMatch ? { idx: codeMatch.index, kind: 'code' as const, m: codeMatch } : null,
      boldMatch ? { idx: boldMatch.index, kind: 'bold' as const, m: boldMatch } : null,
      linkMatch ? { idx: linkMatch.index, kind: 'link' as const, m: linkMatch } : null,
    ].filter(Boolean) as { idx: number; kind: 'code' | 'bold' | 'link'; m: RegExpExecArray }[];

    if (candidates.length === 0) {
      nodes.push(rest);
      break;
    }
    candidates.sort((a, b) => a.idx - b.idx);
    const next = candidates[0];

    if (next.idx > 0) nodes.push(rest.slice(0, next.idx));

    if (next.kind === 'code') {
      nodes.push(
        <code
          key={key++}
          className="font-mono text-[12px] bg-bg-control px-[6px] py-[1px] rounded"
        >
          {next.m[1]}
        </code>,
      );
    } else if (next.kind === 'bold') {
      nodes.push(
        <strong key={key++} className="font-semibold">
          {next.m[1]}
        </strong>,
      );
    } else {
      nodes.push(
        <a
          key={key++}
          href={next.m[2]}
          target="_blank"
          rel="noreferrer"
          className="underline text-text-secondary hover:text-text-primary"
        >
          {next.m[1]}
        </a>,
      );
    }

    rest = rest.slice(next.idx + next.m[0].length);
  }

  return <>{nodes}</>;
}
