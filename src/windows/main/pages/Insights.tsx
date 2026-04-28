import { useEffect, useMemo, useState } from 'react';
import {
  listenTranscriptionSaved,
  usageSummary,
  type UsageSummary,
} from '../../../lib/ipc';

const TYPING_WPM_BASELINE = 40;
const HEATMAP_WEEKS = 34;          // ≈ 8 months
const HEATMAP_PALETTE = [
  '#f0efea', // 0 — heatmap-0 (light mode); CSS var below handles dark mode
  '#dcd9cc',
  '#bcb5a4',
  '#9d9485',
  '#7a7166',
];

export default function Insights() {
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    usageSummary()
      .then((s) => {
        setSummary(s);
        setError(null);
      })
      .catch((e) => setError(String(e)));

  useEffect(() => {
    refresh();
    let unlisten: (() => void) | null = null;
    listenTranscriptionSaved(refresh).then((u) => (unlisten = u));
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  if (error) {
    return (
      <div className="max-w-[760px] mx-auto">
        <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-7">Insights</h1>
        <p className="text-[13px] text-[#e85d4a]">Couldn't load stats: {error}</p>
      </div>
    );
  }

  if (!summary) {
    return (
      <div className="max-w-[760px] mx-auto">
        <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-7">Insights</h1>
        <p className="text-[13px] text-text-quaternary">Loading…</p>
      </div>
    );
  }

  const t = summary.totals;
  const wpm =
    t.total_speech_ms > 0
      ? Math.round((t.total_words * 60_000) / t.total_speech_ms)
      : 0;
  const wpmCompare = wpm > 0
    ? `${(wpm / TYPING_WPM_BASELINE).toFixed(1)}× your typing speed`
    : 'Speak a few sentences to see your speed.';
  const totalWords = t.total_words;
  const wordsCompare = wordTier(totalWords);
  const timeSavedMs = wpm > 0
    ? Math.max(0, ((totalWords / TYPING_WPM_BASELINE) - (t.total_speech_ms / 60_000)) * 60_000)
    : 0;

  return (
    <div className="max-w-[840px] mx-auto">
      <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-7">Insights</h1>

      {/* ---------- Three big stat cards ---------- */}
      <div className="grid grid-cols-3 gap-3 mb-7">
        <BigStat value={wpm.toString()} label="Words per minute" context={wpmCompare} />
        <BigStat
          value={totalWords.toLocaleString()}
          label="Total words dictated"
          context={wordsCompare}
        />
        <BigStat
          value={formatHm(timeSavedMs)}
          label="Time saved"
          context="vs. typing at 40 WPM"
        />
      </div>

      {/* ---------- Streak heatmap ---------- */}
      <Heatmap
        currentStreak={t.current_streak}
        longest={t.longest_streak}
        days={summary.heatmap}
      />

      {/* ---------- Activity + most-used words ---------- */}
      <div className="grid grid-cols-2 gap-3 mt-4">
        <RecentActivity totals={t} />
        <MostUsedWords words={summary.top_words} />
      </div>

      {/* ---------- Filler-words card ---------- */}
      <FillerWordsCard
        fillers={summary.top_fillers}
        totalRemoved={summary.total_fillers_removed}
      />

      {/* ---------- Speaking habits (milestone-gated) ---------- */}
      <h2 className="font-serif text-[22px] tracking-[-0.3px] text-text-primary mt-10 mb-3">
        Speaking habits
      </h2>
      <p className="text-[12px] text-text-quaternary mb-4 leading-[1.55] max-w-[560px]">
        These unlock as you use Murmr more — the more you dictate, the more there is to see.
      </p>

      <div className="grid grid-cols-2 gap-3">
        <PhrasesCard
          phrases={summary.top_phrases}
          unlockAt={30}
          unlocked={t.total_transcriptions >= 30}
          totalSoFar={t.total_transcriptions}
        />
        <HourlyCard
          hourly={summary.hourly}
          unlockAt={20}
          unlocked={t.total_transcriptions >= 20}
          totalSoFar={t.total_transcriptions}
        />
      </div>
      <PersonaCard
        themes={summary.themes}
        totalTranscriptions={t.total_transcriptions}
        avgWords={
          t.total_transcriptions > 0 ? Math.round(t.total_words / t.total_transcriptions) : 0
        }
        wpm={wpm}
        unlockAt={15}
        unlocked={t.total_transcriptions >= 15}
        totalSoFar={t.total_transcriptions}
      />
    </div>
  );
}

// ---------- Milestone-gated cards ----------

function MilestoneShell({
  title,
  children,
  unlocked,
  unlockAt,
  totalSoFar,
  full,
}: {
  title: string;
  children: React.ReactNode;
  unlocked: boolean;
  unlockAt: number;
  totalSoFar: number;
  full?: boolean;
}) {
  return (
    <div
      className={
        'rounded-[12px] bg-bg-row border border-border-hairline p-5 mt-3 ' +
        (full ? '' : '')
      }
    >
      <div className="flex items-baseline justify-between mb-1.5">
        <h3 className="font-serif text-[17px] tracking-[-0.2px] text-text-primary m-0">
          {title}
        </h3>
        {!unlocked && (
          <span className="text-[10px] uppercase tracking-[0.6px] text-text-quaternary font-medium">
            Unlocks at {unlockAt}
          </span>
        )}
      </div>
      {!unlocked ? (
        <LockedBody current={totalSoFar} target={unlockAt} />
      ) : (
        children
      )}
    </div>
  );
}

function LockedBody({ current, target }: { current: number; target: number }) {
  const progress = Math.min(1, current / target);
  return (
    <>
      <p className="text-[12px] text-text-tertiary leading-[1.55] mb-3">
        Surfaces after you've finished {target} dictations.
      </p>
      <div className="h-[6px] rounded-full overflow-hidden bg-bg-control">
        <div
          className="h-full rounded-full bg-text-secondary"
          style={{ width: `${progress * 100}%`, transition: 'width 240ms ease-out' }}
        />
      </div>
      <div className="text-[11px] text-text-quaternary tabular-nums mt-1.5">
        {current} / {target}
      </div>
    </>
  );
}

function PhrasesCard({
  phrases,
  unlockAt,
  unlocked,
  totalSoFar,
}: {
  phrases: Array<{ phrase: string; count: number }>;
  unlockAt: number;
  unlocked: boolean;
  totalSoFar: number;
}) {
  return (
    <MilestoneShell
      title="Phrases you say a lot"
      unlocked={unlocked}
      unlockAt={unlockAt}
      totalSoFar={totalSoFar}
    >
      {phrases.length === 0 ? (
        <p className="text-[12px] text-text-tertiary leading-[1.55]">
          Once you've used a few short phrases multiple times, they'll show up here.
        </p>
      ) : (
        <ul className="space-y-1.5 mt-1">
          {phrases.slice(0, 6).map((p) => (
            <li key={p.phrase} className="flex items-baseline justify-between gap-3">
              <span className="font-serif text-[15px] text-text-primary">"{p.phrase}"</span>
              <span className="text-[11px] text-text-quaternary tabular-nums">
                {p.count}×
              </span>
            </li>
          ))}
        </ul>
      )}
    </MilestoneShell>
  );
}

function HourlyCard({
  hourly,
  unlockAt,
  unlocked,
  totalSoFar,
}: {
  hourly: number[];
  unlockAt: number;
  unlocked: boolean;
  totalSoFar: number;
}) {
  const max = Math.max(...hourly, 1);
  const peakHour = hourly.indexOf(Math.max(...hourly));
  const peakLabel = formatHour(peakHour);

  return (
    <MilestoneShell
      title="When you dictate"
      unlocked={unlocked}
      unlockAt={unlockAt}
      totalSoFar={totalSoFar}
    >
      <p className="text-[12px] text-text-tertiary mb-2.5">
        You dictate most around{' '}
        <span className="font-serif text-text-primary text-[14px]">{peakLabel}</span>.
      </p>
      <div className="flex items-end gap-[2px] h-[42px]">
        {hourly.map((c, i) => {
          const norm = c / max;
          const height = Math.max(2, norm * 42);
          const opacity = i === peakHour ? 1 : 0.45 + norm * 0.4;
          return (
            <div
              key={i}
              className="flex-1 rounded-[2px]"
              style={{
                height,
                background: 'var(--text-secondary)',
                opacity,
              }}
              title={`${formatHour(i)} — ${c} dictation${c === 1 ? '' : 's'}`}
            />
          );
        })}
      </div>
      <div className="flex justify-between text-[10px] text-text-quaternary mt-1.5">
        <span>12am</span>
        <span>6am</span>
        <span>noon</span>
        <span>6pm</span>
      </div>
    </MilestoneShell>
  );
}

function PersonaCard({
  themes,
  totalTranscriptions,
  avgWords,
  wpm,
  unlockAt,
  unlocked,
  totalSoFar,
}: {
  themes: Array<{
    theme: string;
    label: string;
    transcription_count: number;
    sample_words: string[];
  }>;
  totalTranscriptions: number;
  avgWords: number;
  wpm: number;
  unlockAt: number;
  unlocked: boolean;
  totalSoFar: number;
}) {
  return (
    <MilestoneShell
      title="What you talk about"
      unlocked={unlocked}
      unlockAt={unlockAt}
      totalSoFar={totalSoFar}
      full
    >
      {themes.length === 0 ? (
        <p className="text-[13px] text-text-tertiary leading-[1.55]">
          We don't have enough yet to draw a picture. Once you've dictated about a few different
          things, the themes you talk about most will surface here.
        </p>
      ) : (
        <Persona
          themes={themes}
          totalTranscriptions={totalTranscriptions}
          avgWords={avgWords}
          wpm={wpm}
        />
      )}
    </MilestoneShell>
  );
}

function Persona({
  themes,
  totalTranscriptions,
  avgWords,
  wpm,
}: {
  themes: Array<{
    theme: string;
    label: string;
    transcription_count: number;
    sample_words: string[];
  }>;
  totalTranscriptions: number;
  avgWords: number;
  wpm: number;
}) {
  const top = themes[0];
  const second = themes[1];
  const third = themes[2];

  // Map theme id → an adjective + a noun so the persona blurb feels personal
  // rather than category-list-y.
  const PROFILE: Record<string, { archetype: string; verb: string }> = {
    building: { archetype: 'a builder', verb: "you're shipping things" },
    coordinating: { archetype: 'a connector', verb: "you're aligning people" },
    planning: { archetype: 'a planner', verb: "you're mapping out what's next" },
    personal: { archetype: 'a homebody', verb: "your life is the throughline" },
    writing: { archetype: 'a writer', verb: "you think on the page" },
    money: { archetype: 'an operator', verb: "you keep the books in mind" },
    travel: { archetype: 'on the move', verb: "you're rarely in one place" },
    leisure: { archetype: 'a curator', verb: 'culture is part of the rhythm' },
    health: { archetype: 'an athlete', verb: "you're tracking the body" },
    errands: { archetype: 'an organizer', verb: 'small tasks add up' },
  };

  const profile = PROFILE[top.theme] ?? { archetype: 'a thinker', verb: 'you cover broad ground' };

  // "Variety" — how spread out are dictations across themes?
  const totalCovered = themes.reduce((sum, t) => sum + t.transcription_count, 0);
  const topShare = totalCovered > 0 ? top.transcription_count / totalCovered : 0;
  const variety = topShare < 0.4 ? 'broad-ranging' : topShare < 0.6 ? 'with a clear lean' : 'pretty focused';

  // "Style" — based on average words per dictation.
  const style =
    avgWords < 12
      ? 'short, to-the-point'
      : avgWords < 30
        ? 'a comfortable middle length'
        : 'longer, more thinking-out-loud';

  return (
    <div>
      <p className="font-serif text-[15px] text-text-primary leading-[1.6] m-0 mb-4">
        You're <strong className="font-medium">{profile.archetype}</strong> — {profile.verb}.
        {second && (
          <>
            {' '}
            Beyond that, you spend time on{' '}
            <span className="text-text-secondary">{second.label.toLowerCase()}</span>
            {third ? (
              <>
                {' '}
                and <span className="text-text-secondary">{third.label.toLowerCase()}</span>
              </>
            ) : null}
            .
          </>
        )}{' '}
        Your dictations are {style} — {variety}.
      </p>

      <div className="grid grid-cols-2 gap-x-5 gap-y-3 mt-3">
        {themes.slice(0, 6).map((t) => {
          const share = totalTranscriptions > 0
            ? Math.round((t.transcription_count / totalTranscriptions) * 100)
            : 0;
          return (
            <div key={t.theme} className="min-w-0">
              <div className="flex items-baseline justify-between gap-3 mb-1">
                <span className="text-[13px] text-text-primary font-medium truncate">
                  {t.label}
                </span>
                <span className="text-[11px] text-text-quaternary tabular-nums">
                  {share}% · {t.transcription_count}
                </span>
              </div>
              <div className="text-[12px] text-text-tertiary leading-[1.4] truncate">
                {t.sample_words.length > 0
                  ? t.sample_words.map((w) => `"${w}"`).join(' · ')
                  : <span className="italic text-text-quaternary">no samples yet</span>}
              </div>
            </div>
          );
        })}
      </div>

      <p className="text-[11px] text-text-quaternary mt-5 leading-[1.5] italic">
        Keyword-based — gets sharper as you talk more. A future update will swap this for a local
        AI rewrite that reads what you've said and writes a richer profile.
      </p>
      {/* `wpm` is in scope — kept for future expansion of the persona blurb */}
      {wpm < 0 && null}
    </div>
  );
}

function formatHour(h: number): string {
  if (h === 0) return '12am';
  if (h === 12) return 'noon';
  if (h < 12) return `${h}am`;
  return `${h - 12}pm`;
}


function FillerWordsCard({
  fillers,
  totalRemoved,
}: {
  fillers: Array<{ word: string; count: number }>;
  totalRemoved: number;
}) {
  const top = fillers[0]?.count ?? 1;
  const palette = ['#7a7166', '#9d9485', '#bcb5a4', '#dcd9cc', '#dcd9cc'];

  return (
    <div className="mt-4 rounded-card bg-bg-row border border-border-hairline p-6">
      <div className="flex items-baseline justify-between mb-1.5">
        <h2 className="font-serif text-[20px] tracking-[-0.3px] text-text-primary m-0">
          Filler words you've used most
        </h2>
        <span className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium">
          Cleaned up · {totalRemoved.toLocaleString()} time{totalRemoved === 1 ? '' : 's'}
        </span>
      </div>
      <p className="text-[12px] text-text-quaternary italic mb-4">
        Murmr quietly removed these so you didn't have to.
      </p>

      {fillers.length === 0 ? (
        <p className="text-[12px] text-text-tertiary leading-[1.55]">
          No filler words removed yet — once Murmr trims your first "um" or "uh", the top five will
          surface here.
        </p>
      ) : (
        <div>
          {fillers.map((f, i) => {
            const ratio = f.count / Math.max(top, 1);
            return (
              <div key={f.word} className="flex items-center gap-3.5 py-[10px]">
                <span className="font-serif text-[14px] text-text-quaternary w-[18px] tabular-nums">
                  {i + 1}
                </span>
                <span className="font-serif text-[17px] text-text-primary w-[100px]">
                  {f.word}
                </span>
                <div className="flex-1 h-[6px] rounded-full overflow-hidden bg-bg-control">
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${Math.max(8, ratio * 100)}%`,
                      background: palette[Math.min(i, palette.length - 1)],
                    }}
                  />
                </div>
                <span className="font-serif text-[15px] text-text-primary tabular-nums w-[44px] text-right">
                  {f.count}
                </span>
              </div>
            );
          })}

          <div className="flex items-center gap-2.5 px-4 py-3 mt-4 rounded-row bg-bg-chrome border border-border-hairline">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--text-quaternary)" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
            <span className="text-[12px] text-text-secondary leading-[1.5]">
              If your transcripts feel <em className="font-serif italic">too</em> polished, edit the
              filler list in{' '}
              <strong className="text-text-primary font-medium">Preferences</strong>.
            </span>
          </div>
        </div>
      )}
    </div>
  );
}

// ---------- Subcomponents ----------

function BigStat({ value, label, context }: { value: string; label: string; context: string }) {
  return (
    <div className="rounded-[12px] bg-bg-row border border-border-hairline p-[22px]">
      <div className="font-serif text-[36px] tracking-[-0.7px] leading-none text-text-primary">
        {value}
      </div>
      <div className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium mt-[6px]">
        {label}
      </div>
      <div className="text-[12px] text-text-secondary mt-[10px] min-h-[18px]">{context}</div>
    </div>
  );
}

function Heatmap({
  currentStreak,
  longest,
  days,
}: {
  currentStreak: number;
  longest: number;
  days: Array<{ day: number; count: number }>;
}) {
  // Build a [7 rows × HEATMAP_WEEKS cols] grid filled from `days`. Today
  // sits at the bottom-right; earlier dates fan up and to the left.
  const grid = useMemo(() => buildHeatmapGrid(days), [days]);

  return (
    <div className="rounded-[12px] bg-bg-row border border-border-hairline p-[24px]">
      <div className="flex items-baseline justify-between mb-[6px]">
        <h2 className="font-serif text-[20px] tracking-[-0.3px] text-text-primary m-0">
          {currentStreak} day streak
        </h2>
        <span className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium">
          Longest · {longest} days
        </span>
      </div>
      <div
        className="grid gap-[3px] mt-3"
        style={{
          gridTemplateColumns: `repeat(${HEATMAP_WEEKS}, 1fr)`,
          gridTemplateRows: 'repeat(7, 1fr)',
          gridAutoFlow: 'column',
        }}
      >
        {grid.map((cell, i) => (
          <span
            key={i}
            title={cell.title}
            className="heatmap-cell aspect-square rounded-[2px]"
            style={{ background: HEATMAP_PALETTE[cell.bucket] }}
          />
        ))}
      </div>
      <style>{`
        .heatmap-cell {
          cursor: pointer;
          transition: transform 80ms ease-out, box-shadow 80ms ease-out;
        }
        .heatmap-cell:hover {
          transform: scale(1.6);
          box-shadow: 0 0 0 1px rgba(0,0,0,0.18), 0 4px 10px rgba(0,0,0,0.10);
          z-index: 1;
          position: relative;
        }
        [data-theme='dark'] .heatmap-cell:hover {
          box-shadow: 0 0 0 1px rgba(255,255,255,0.20), 0 4px 10px rgba(0,0,0,0.40);
        }
      `}</style>
      <div className="flex items-center justify-end gap-2 mt-3 text-[10px] text-text-quaternary">
        Less
        {HEATMAP_PALETTE.map((color, i) => (
          <span
            key={i}
            className="inline-block w-[10px] h-[10px] rounded-[2px]"
            style={{ background: color }}
          />
        ))}
        More
      </div>
    </div>
  );
}

function RecentActivity({
  totals,
}: {
  totals: UsageSummary['totals'];
}) {
  const avgWords =
    totals.total_transcriptions > 0
      ? Math.round(totals.total_words / totals.total_transcriptions)
      : 0;
  const avgWpm =
    totals.total_speech_ms > 0
      ? Math.round((totals.total_words * 60_000) / totals.total_speech_ms)
      : 0;

  return (
    <div className="rounded-[12px] bg-bg-row border border-border-hairline p-[22px]">
      <h2 className="font-serif text-[18px] tracking-[-0.2px] text-text-primary m-0 mb-3">
        Recent activity
      </h2>
      <ActivityRow label="Transcriptions" value={totals.total_transcriptions.toLocaleString()} />
      <ActivityRow label="Words spoken" value={totals.total_words.toLocaleString()} />
      <ActivityRow label="Speech duration" value={formatHm(totals.total_speech_ms)} />
      <ActivityRow label="Avg length" value={`${avgWords} words`} />
      <ActivityRow label="Avg pace" value={`${avgWpm} WPM`} />
    </div>
  );
}

function ActivityRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-[5px] text-[12px]">
      <span className="text-text-tertiary">{label}</span>
      <span className="text-text-primary tabular-nums font-medium">{value}</span>
    </div>
  );
}

function MostUsedWords({ words }: { words: Array<{ word: string; count: number }> }) {
  return (
    <div className="rounded-[12px] bg-bg-row border border-border-hairline p-[22px]">
      <h2 className="font-serif text-[18px] tracking-[-0.2px] text-text-primary m-0 mb-3">
        Most-used words
      </h2>
      {words.length === 0 ? (
        <p className="text-[12px] text-text-quaternary">
          A few transcriptions in and the most-used words will surface here.
        </p>
      ) : (
        <div className="flex flex-wrap gap-[6px]">
          {words.map((w) => (
            <span
              key={w.word}
              className="inline-flex items-center gap-[6px] px-[10px] py-[4px] rounded-full bg-bg-control text-[12px] text-text-primary"
            >
              <span className="font-medium">{w.word}</span>
              <span className="text-text-tertiary tabular-nums">{w.count}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------- Helpers ----------

function buildHeatmapGrid(
  days: Array<{ day: number; count: number }>,
): Array<{ bucket: number; title: string }> {
  // Map day → count for fast lookup.
  const counts = new Map<number, number>();
  for (const d of days) counts.set(d.day, d.count);

  // Today's date in yyyymmdd (local, 4am-shifted to match the backend).
  const todayInt = todayWith4amOffset();

  const cells: Array<{ bucket: number; title: string }> = [];
  // We render columns left-to-right (oldest week first), rows top-to-bottom
  // (Mon–Sun). The latest column ends at "today's day-of-week" row.
  const totalCells = HEATMAP_WEEKS * 7;

  // Compute the max count to bucket against.
  let maxCount = 0;
  for (const c of counts.values()) if (c > maxCount) maxCount = c;

  for (let i = 0; i < totalCells; i++) {
    // i counts cells column-major (per gridAutoFlow: column).
    const dayOffsetFromToday = totalCells - 1 - i;
    const dayInt = subtractDays(todayInt, dayOffsetFromToday);
    const count = counts.get(dayInt) ?? 0;
    cells.push({
      bucket: bucketFor(count, maxCount),
      title: `${formatDayInt(dayInt)} — ${count} ${count === 1 ? 'transcription' : 'transcriptions'}`,
    });
  }
  return cells;
}

function bucketFor(count: number, max: number): number {
  if (count <= 0) return 0;
  if (max <= 1) return count > 0 ? 4 : 0;
  const ratio = count / max;
  if (ratio < 0.25) return 1;
  if (ratio < 0.5) return 2;
  if (ratio < 0.75) return 3;
  return 4;
}

function todayWith4amOffset(): number {
  const now = new Date(Date.now() - 4 * 60 * 60 * 1000);
  return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}

function subtractDays(yyyymmdd: number, n: number): number {
  // Walk back via JS Date, normalized to local midday to avoid DST jitter.
  const y = Math.floor(yyyymmdd / 10000);
  const m = Math.floor((yyyymmdd / 100) % 100) - 1;
  const d = yyyymmdd % 100;
  const dt = new Date(y, m, d, 12, 0, 0);
  dt.setDate(dt.getDate() - n);
  return dt.getFullYear() * 10000 + (dt.getMonth() + 1) * 100 + dt.getDate();
}

function formatDayInt(yyyymmdd: number): string {
  const y = Math.floor(yyyymmdd / 10000);
  const m = Math.floor((yyyymmdd / 100) % 100);
  const d = yyyymmdd % 100;
  return `${y}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
}

function formatHm(ms: number): string {
  const totalMin = Math.round(ms / 60_000);
  if (totalMin < 60) return `${totalMin}m`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

function wordTier(words: number): string {
  if (words === 0) return 'Speak some words to fill this in.';
  if (words < 200) return '~a long email';
  if (words < 1_000) return '~3 short emails worth';
  if (words < 5_000) return '~10 short emails worth';
  if (words < 20_000) return '~a short story worth';
  if (words < 100_000) return '~a novella worth';
  return '~a novel worth';
}
