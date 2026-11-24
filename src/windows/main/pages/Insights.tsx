import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  listenTranscriptionSaved,
  usageSummary,
  type AppUsage,
  type FillerProgress,
  type PersonalRecords,
  type Transcription,
  type UsageSummary,
  type WeekWpm,
} from '../../../lib/ipc';
import './insights.css';

const TYPING_WPM_BASELINE = 40;
const HEATMAP_WEEKS = 34; // ≈ 8 months
const HEATMAP_LEVELS = [
  'var(--heatmap-0)',
  'var(--heatmap-1)',
  'var(--heatmap-2)',
  'var(--heatmap-3)',
  'var(--heatmap-4)',
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
      <div>
        <PageHeader />
        <p className="text-[13px] text-danger">Couldn't load stats: {error}</p>
      </div>
    );
  }

  if (!summary) {
    return (
      <div>
        <PageHeader />
        <p className="text-[13px] text-text-quaternary">Loading…</p>
      </div>
    );
  }

  const t = summary.totals;
  const wpm = t.total_speech_ms > 0 ? Math.round((t.total_words * 60_000) / t.total_speech_ms) : 0;
  const totalWords = t.total_words;
  const timeSavedMs =
    wpm > 0
      ? Math.max(0, (totalWords / TYPING_WPM_BASELINE - t.total_speech_ms / 60_000) * 60_000)
      : 0;
  const avgWords = t.total_transcriptions > 0 ? Math.round(t.total_words / t.total_transcriptions) : 0;

  return (
    <div className="insights flex flex-col gap-[14px]">
      <PageHeader count={t.total_transcriptions} />

      {/* ---------- Hero: speed + two big numbers ---------- */}
      <div className="grid gap-[14px] fade-up" style={{ gridTemplateColumns: '1.55fr 1fr' }}>
        <SpeedHero wpm={wpm} weekly={summary.weekly_wpm} />
        <div className="grid grid-rows-2 gap-[14px]">
          <BigTile
            label="Words dictated"
            value={totalWords.toLocaleString()}
            context={wordTier(totalWords)}
          />
          <BigTile
            label="Time saved"
            value={wpm > 0 ? formatHm(timeSavedMs) : '—'}
            context={`vs. typing at ${TYPING_WPM_BASELINE} wpm`}
          />
        </div>
      </div>

      {/* ---------- Stat strip ---------- */}
      <StatStrip totals={t} avgWords={avgWords} weekly={summary.weekly_wpm} />

      {/* ---------- Streak + heatmap ---------- */}
      <StreakCard currentStreak={t.current_streak} longest={t.longest_streak} days={summary.heatmap} />

      {/* ---------- When / where ---------- */}
      <div className="grid grid-cols-2 gap-[14px]">
        <HourlyCard
          hourly={summary.hourly}
          unlockAt={20}
          unlocked={t.total_transcriptions >= 20}
          totalSoFar={t.total_transcriptions}
        />
        <AppBreakdownCard apps={summary.app_breakdown} />
      </div>

      {/* ---------- Personal records ---------- */}
      <PersonalRecordsRow records={summary.personal_records} />

      {/* ---------- Words + fillers ---------- */}
      <div className="grid grid-cols-2 gap-[14px]">
        <MostUsedWords words={summary.top_words} />
        <FillerWordsCard
          fillers={summary.top_fillers}
          totalRemoved={summary.total_fillers_removed}
          progress={summary.filler_progress}
        />
      </div>

      {/* ---------- Speaking habits (milestone-gated) ---------- */}
      <div className="grid grid-cols-2 gap-[14px]">
        <PhrasesCard
          phrases={summary.top_phrases}
          unlockAt={30}
          unlocked={t.total_transcriptions >= 30}
          totalSoFar={t.total_transcriptions}
        />
        <PersonaCard
          themes={summary.themes}
          totalTranscriptions={t.total_transcriptions}
          avgWords={avgWords}
          unlockAt={15}
          unlocked={t.total_transcriptions >= 15}
          totalSoFar={t.total_transcriptions}
        />
      </div>
    </div>
  );
}

// ---------- Layout pieces ----------

function PageHeader({ count }: { count?: number }) {
  return (
    <div className="flex items-end justify-between mb-[14px] fade-up">
      <div>
        <h1 className="page-title">Insights</h1>
        <p className="text-[13px] text-text-tertiary mt-[6px]">
          Updated after every dictation. Nothing leaves this computer.
        </p>
      </div>
      {count !== undefined && (
        <div className="text-[12px] text-text-secondary">
          <span className="num">{count.toLocaleString()}</span> dictation{count === 1 ? '' : 's'}
        </div>
      )}
    </div>
  );
}

function Panel({
  children,
  className = '',
  style,
}: {
  children: ReactNode;
  className?: string;
  style?: React.CSSProperties;
}) {
  return (
    <section className={`glass overflow-hidden ${className}`} style={style}>
      {children}
    </section>
  );
}

function PanelTitle({ title, sub, right }: { title: string; sub?: ReactNode; right?: ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary m-0">{title}</h2>
        {sub && <p className="text-[12px] text-text-tertiary mt-[2px]">{sub}</p>}
      </div>
      {right}
    </div>
  );
}

/// Shared hover tooltip for the charts. Positioned inside a relative parent.
interface Tip {
  x: number;
  y: number;
  content: ReactNode;
}

function ChartTip({ tip }: { tip: Tip | null }) {
  if (!tip) return null;
  return (
    <div className="chart-tip" style={{ left: tip.x, top: tip.y }} role="status">
      {tip.content}
    </div>
  );
}

function useWidth<T extends HTMLElement>(): [React.RefObject<T | null>, number] {
  const ref = useRef<T | null>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, width];
}

// ---------- Hero ----------

function SpeedHero({ wpm, weekly }: { wpm: number; weekly: WeekWpm[] }) {
  const factor = wpm > 0 ? wpm / TYPING_WPM_BASELINE : 0;
  // Drop trailing empty weeks (nothing dictated yet this week) but keep
  // gaps that sit between real data — an honest picture.
  const trimmed = useMemo(() => {
    let end = weekly.length;
    while (end > 0 && weekly[end - 1].words === 0) end--;
    return weekly.slice(0, end);
  }, [weekly]);
  const points = trimmed.filter((w) => w.avg_wpm !== null && w.words > 0);
  const latest = points[points.length - 1];

  return (
    <Panel className="p-6 min-h-[258px]">
      <div className="hero-glow" aria-hidden="true" />
      <div className="relative">
        <div className="eyebrow">Speaking speed</div>
        <div className="flex items-baseline gap-[10px] mt-[6px]">
          <span className="num text-[88px] font-semibold tracking-[-4px] leading-[0.95] text-text-primary">
            {wpm > 0 ? wpm : '—'}
          </span>
          <span className="text-[18px] text-text-secondary font-medium">wpm</span>
          {wpm > 0 && (
            <span className="ml-auto self-center chip chip-accent !h-[28px] !text-[12px] !font-semibold">
              {factor.toFixed(1)}× faster than typing
            </span>
          )}
        </div>
        {points.length >= 2 ? (
          <>
            <WpmChart weeks={trimmed} />
            <div className="flex justify-between text-[11px] text-text-tertiary mt-[6px]">
              <span>{trimmed.length} weeks ago</span>
              <span>
                Weekly average · this week{' '}
                <span className="num text-text-secondary">{Math.round(latest.avg_wpm ?? 0)}</span>
              </span>
            </div>
          </>
        ) : (
          <p className="text-[12px] text-text-tertiary mt-6">
            {wpm > 0
              ? 'Your weekly pace chart fills in after a couple of weeks of dictating.'
              : 'Speak a few sentences to see your speed.'}
          </p>
        )}
      </div>
    </Panel>
  );
}

function WpmChart({ weeks }: { weeks: WeekWpm[] }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const height = 84;
  const padTop = 8;
  const padBottom = 2;

  const geom = useMemo(() => {
    const vals = weeks.map((w) => (w.avg_wpm !== null && w.words > 0 ? w.avg_wpm : null));
    const real = vals.filter((v): v is number => v !== null);
    if (real.length === 0 || width === 0) return null;
    const max = Math.max(...real);
    const min = Math.min(...real);
    const range = Math.max(8, max - min);
    const lo = min - range * 0.15;
    const hi = max + range * 0.1;
    const stepX = weeks.length > 1 ? width / (weeks.length - 1) : 0;
    const pts = vals.map((v, i) =>
      v === null
        ? null
        : {
            x: i * stepX,
            y: padTop + (1 - (v - lo) / (hi - lo)) * (height - padTop - padBottom),
            v,
            i,
          },
    );
    // Split into runs so gaps stay gaps.
    const runs: Array<Array<{ x: number; y: number }>> = [];
    let cur: Array<{ x: number; y: number }> = [];
    for (const p of pts) {
      if (p) cur.push(p);
      else if (cur.length) {
        runs.push(cur);
        cur = [];
      }
    }
    if (cur.length) runs.push(cur);
    const line = runs
      .map((r) => r.map((p, j) => `${j === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' '))
      .join(' ');
    const area = runs
      .filter((r) => r.length > 1)
      .map(
        (r) =>
          `M${r[0].x.toFixed(1)},${height} ` +
          r.map((p) => `L${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ') +
          ` L${r[r.length - 1].x.toFixed(1)},${height} Z`,
      )
      .join(' ');
    return { pts, line, area, stepX };
  }, [weeks, width]);

  const last = geom?.pts.filter(Boolean).slice(-1)[0] ?? null;
  const hp = hover !== null ? geom?.pts[hover] ?? null : null;

  const onMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!geom || geom.stepX === 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const i = Math.round((e.clientX - rect.left) / geom.stepX);
    const clamped = Math.max(0, Math.min(weeks.length - 1, i));
    setHover(geom.pts[clamped] ? clamped : null);
  };

  return (
    <div
      ref={ref}
      className="relative mt-[14px]"
      style={{ height }}
      onMouseMove={onMove}
      onMouseLeave={() => setHover(null)}
      role="img"
      aria-label={`Weekly average words per minute over the last ${weeks.length} weeks`}
    >
      {geom && (
        <svg width={width} height={height} className="block overflow-visible">
          <defs>
            <linearGradient id="wpm-fill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="var(--accent)" stopOpacity="0.32" />
              <stop offset="1" stopColor="var(--accent)" stopOpacity="0" />
            </linearGradient>
          </defs>
          <line x1="0" y1={height - 0.5} x2={width} y2={height - 0.5} stroke="var(--border-hairline)" />
          <path d={geom.area} fill="url(#wpm-fill)" />
          <path
            d={geom.line}
            fill="none"
            stroke="var(--accent)"
            strokeWidth="2"
            strokeLinejoin="round"
            strokeLinecap="round"
          />
          {hp && (
            <line
              x1={hp.x}
              x2={hp.x}
              y1={0}
              y2={height}
              stroke="var(--border-strong)"
              strokeDasharray="2 3"
            />
          )}
          {(hp ?? last) && (
            <circle
              cx={(hp ?? last)!.x}
              cy={(hp ?? last)!.y}
              r="5"
              fill="var(--accent)"
              stroke="var(--ground)"
              strokeWidth="2"
            />
          )}
        </svg>
      )}
      {hp && (
        <ChartTip
          tip={{
            x: hp.x,
            y: hp.y - 10,
            content: (
              <>
                <div className="text-text-tertiary">Week of {formatWeek(weeks[hp.i].week_start_ms)}</div>
                <div className="num font-semibold text-text-primary">{Math.round(hp.v)} wpm</div>
              </>
            ),
          }}
        />
      )}
    </div>
  );
}

function BigTile({ label, value, context }: { label: string; value: string; context: string }) {
  return (
    <Panel className="px-[22px] py-5 flex flex-col justify-center">
      <div className="eyebrow">{label}</div>
      <div className="num text-[40px] font-semibold tracking-[-1.6px] leading-[1.1] mt-[6px] text-text-primary">
        {value}
      </div>
      <div className="text-[12px] text-text-secondary mt-[2px]">{context}</div>
    </Panel>
  );
}

function StatStrip({
  totals,
  avgWords,
  weekly,
}: {
  totals: UsageSummary['totals'];
  avgWords: number;
  weekly: WeekWpm[];
}) {
  const withData = weekly.filter((w) => w.words > 0);
  const thisWeek = weekly.length > 0 ? weekly[weekly.length - 1].words : 0;
  const a = withData[withData.length - 1]?.avg_wpm;
  const b = withData[withData.length - 2]?.avg_wpm;
  const delta = a != null && b != null && b > 0 ? ((a - b) / b) * 100 : null;

  const stats: Array<{ label: string; value: string }> = [
    { label: 'Dictations', value: totals.total_transcriptions.toLocaleString() },
    { label: 'Speaking time', value: formatHm(totals.total_speech_ms) },
    { label: 'Avg length', value: `${avgWords} words` },
    { label: 'This week', value: `${thisWeek.toLocaleString()} words` },
    {
      label: 'Pace vs last week',
      value: delta === null ? '—' : `${delta > 0 ? '+' : ''}${delta.toFixed(0)}%`,
    },
  ];
  return (
    <Panel className="px-6 py-4 grid grid-cols-5 gap-3 fade-up">
      {stats.map((s) => (
        <div key={s.label} className="min-w-0">
          <div className="text-[11px] text-text-tertiary truncate">{s.label}</div>
          <div className="num text-[17px] font-semibold tracking-[-0.3px] mt-[3px] text-text-primary truncate">
            {s.value}
          </div>
        </div>
      ))}
    </Panel>
  );
}

// ---------- Streak + heatmap ----------

function StreakCard({
  currentStreak,
  longest,
  days,
}: {
  currentStreak: number;
  longest: number;
  days: Array<{ day: number; count: number }>;
}) {
  const grid = useMemo(() => buildHeatmapGrid(days), [days]);
  const [tip, setTip] = useState<Tip | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  const months = useMemo(() => {
    const out: Array<{ col: number; label: string }> = [];
    let prev = -1;
    for (let col = 0; col < HEATMAP_WEEKS; col++) {
      const cell = grid[col * 7];
      const m = Math.floor((cell.dayInt / 100) % 100);
      if (m !== prev) {
        // Skip a label crammed into the first two columns of a partial month.
        if (!(col <= 1 && prev === -1 && Math.floor((grid[(col + 2) * 7]?.dayInt / 100) % 100) !== m)) {
          out.push({ col, label: MONTHS[m - 1] });
        }
        prev = m;
      }
    }
    return out;
  }, [grid]);

  const show = (e: React.MouseEvent<HTMLSpanElement>, i: number) => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const r = e.currentTarget.getBoundingClientRect();
    const w = wrap.getBoundingClientRect();
    const c = grid[i];
    setTip({
      x: r.left - w.left + r.width / 2,
      y: r.top - w.top - 6,
      content: (
        <>
          <div className="text-text-tertiary">{formatDayLong(c.dayInt)}</div>
          <div className="num font-semibold text-text-primary">
            {c.count} {c.count === 1 ? 'dictation' : 'dictations'}
          </div>
        </>
      ),
    });
  };

  return (
    <Panel className="px-6 py-[22px] flex gap-7 items-stretch">
      <div className="w-[170px] flex-none flex flex-col">
        <div className="eyebrow">Streak</div>
        <div className="flex items-baseline gap-2 mt-2">
          <span className="num text-[64px] font-semibold tracking-[-3px] leading-none text-text-primary">
            {currentStreak}
          </span>
          <span className="text-[14px] text-text-secondary">{currentStreak === 1 ? 'day' : 'days'}</span>
        </div>
        <div className="text-[12px] text-text-secondary mt-2">
          Longest: <span className="num">{longest}</span> {longest === 1 ? 'day' : 'days'}
        </div>
        <div className="mt-auto pt-3 text-[11px] text-text-tertiary">
          {currentStreak > 0 ? 'Dictate today to keep it going.' : 'Dictate today to start a streak.'}
        </div>
      </div>
      <div className="flex-1 min-w-0">
        <div className="relative h-[14px] mb-2 text-[11px] text-text-tertiary">
          {months.map((m) => (
            <span
              key={`${m.col}-${m.label}`}
              className="absolute top-0"
              style={{ left: `${(m.col / HEATMAP_WEEKS) * 100}%` }}
            >
              {m.label}
            </span>
          ))}
        </div>
        <div ref={wrapRef} className="relative" onMouseLeave={() => setTip(null)}>
          <div
            className="grid gap-[3px]"
            style={{
              gridTemplateColumns: `repeat(${HEATMAP_WEEKS}, 1fr)`,
              gridTemplateRows: 'repeat(7, auto)',
              gridAutoFlow: 'column',
            }}
          >
            {grid.map((cell, i) => (
              <span
                key={i}
                aria-label={`${formatDayLong(cell.dayInt)}: ${cell.count} dictations`}
                onMouseEnter={(e) => show(e, i)}
                className={'heat-cell' + (cell.isToday ? ' heat-today' : '') + (cell.future ? ' heat-future' : '')}
                style={{ background: cell.future ? 'transparent' : HEATMAP_LEVELS[cell.bucket] }}
              />
            ))}
          </div>
          <ChartTip tip={tip} />
        </div>
        <div className="flex items-center justify-end gap-1 mt-[10px] text-[11px] text-text-tertiary">
          Less
          {HEATMAP_LEVELS.map((color, i) => (
            <span key={i} className="inline-block w-[11px] h-[11px] rounded-[3px]" style={{ background: color }} />
          ))}
          More
        </div>
      </div>
    </Panel>
  );
}

// ---------- When / where ----------

function MilestoneShell({
  title,
  sub,
  children,
  unlocked,
  unlockAt,
  totalSoFar,
}: {
  title: string;
  sub?: ReactNode;
  children: ReactNode;
  unlocked: boolean;
  unlockAt: number;
  totalSoFar: number;
}) {
  return (
    <Panel className="px-6 py-[22px]">
      <PanelTitle
        title={title}
        sub={unlocked ? sub : undefined}
        right={
          !unlocked ? (
            <span className="chip !h-[22px] !text-[10.5px] whitespace-nowrap">
              <LockIcon /> Unlocks at {unlockAt}
            </span>
          ) : undefined
        }
      />
      {!unlocked ? <LockedBody current={totalSoFar} target={unlockAt} /> : children}
    </Panel>
  );
}

function LockedBody({ current, target }: { current: number; target: number }) {
  const progress = Math.min(1, current / target);
  return (
    <div className="mt-4">
      <p className="text-[12px] text-text-tertiary leading-[1.55] mb-3">
        Shows up after you've finished {target} dictations.
      </p>
      <div className="h-[6px] rounded-full overflow-hidden bg-bg-control">
        <div
          className="h-full rounded-full bg-accent"
          style={{ width: `${progress * 100}%`, transition: 'width 240ms ease-out' }}
        />
      </div>
      <div className="text-[11px] text-text-quaternary num mt-[6px]">
        {current} / {target}
      </div>
    </div>
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
  const [tip, setTip] = useState<Tip | null>(null);
  const barsRef = useRef<HTMLDivElement | null>(null);
  const barHeight = 104;

  return (
    <MilestoneShell
      title="When you dictate"
      sub={<>Busiest around {formatHourLong(peakHour)}</>}
      unlocked={unlocked}
      unlockAt={unlockAt}
      totalSoFar={totalSoFar}
    >
      <div
        ref={barsRef}
        className="relative flex items-end gap-[3px] mt-[18px] border-b border-border-hairline"
        style={{ height: barHeight }}
        onMouseLeave={() => setTip(null)}
      >
        {hourly.map((c, i) => {
          const h = c > 0 ? Math.max(3, (c / max) * (barHeight - 6)) : 2;
          return (
            <div
              key={i}
              className="flex-1 h-full flex items-end cursor-default"
              onMouseEnter={(e) => {
                const wrap = barsRef.current?.getBoundingClientRect();
                const r = e.currentTarget.getBoundingClientRect();
                if (!wrap) return;
                setTip({
                  x: r.left - wrap.left + r.width / 2,
                  y: barHeight - h - 8,
                  content: (
                    <>
                      <div className="text-text-tertiary">{formatHourRange(i)}</div>
                      <div className="num font-semibold text-text-primary">
                        {c} {c === 1 ? 'dictation' : 'dictations'}
                      </div>
                    </>
                  ),
                });
              }}
            >
              <span
                className="block w-full rounded-t-[4px] transition-opacity"
                style={{
                  height: h,
                  background: i === peakHour ? 'var(--accent)' : 'var(--bar-neutral)',
                  opacity: c === 0 ? 0.5 : 1,
                }}
              />
            </div>
          );
        })}
        <ChartTip tip={tip} />
      </div>
      <div className="flex justify-between text-[11px] text-text-tertiary mt-[6px]">
        <span>12a</span>
        <span>6a</span>
        <span>12p</span>
        <span>6p</span>
        <span>12a</span>
      </div>
    </MilestoneShell>
  );
}

function AppBreakdownCard({ apps }: { apps: AppUsage[] }) {
  const total = apps.reduce((sum, a) => sum + a.transcription_count, 0);
  return (
    <Panel className="px-6 py-[22px]">
      <PanelTitle title="Where you dictate" sub="Share of dictations by app" />
      {apps.length === 0 ? (
        <p className="text-[12px] text-text-tertiary mt-4 leading-[1.55]">
          Once you've used Murmr in a few apps, the breakdown shows up here.
        </p>
      ) : (
        <div className="flex flex-col gap-[11px] mt-4">
          {apps.slice(0, 6).map((a) => {
            const pct = total > 0 ? Math.round((a.transcription_count / total) * 100) : 0;
            return (
              <div
                key={a.app}
                className="grid items-center gap-[10px] text-[12px]"
                style={{ gridTemplateColumns: 'minmax(0, 110px) 1fr 36px' }}
                title={`${formatAppName(a.app)}: ${a.transcription_count.toLocaleString()} dictations`}
              >
                <span className="text-text-secondary truncate">{formatAppName(a.app)}</span>
                <span className="h-2 rounded-[4px] bg-bg-control overflow-hidden">
                  <span
                    className="block h-full rounded-[4px]"
                    style={{ width: `${Math.max(2, pct)}%`, background: 'var(--bar-neutral)' }}
                  />
                </span>
                <span className="num text-text-tertiary text-right">{pct}%</span>
              </div>
            );
          })}
        </div>
      )}
    </Panel>
  );
}

// ---------- Personal records ----------

function PersonalRecordsRow({ records }: { records: PersonalRecords }) {
  const tiles: Array<{ label: string; value: string; item: Transcription | null }> = [
    {
      label: 'Longest dictation',
      value: records.longest_words ? `${records.longest_words.word_count.toLocaleString()} words` : '—',
      item: records.longest_words,
    },
    {
      label: 'Longest take',
      value: records.longest_duration ? formatRecordDuration(records.longest_duration.duration_ms) : '—',
      item: records.longest_duration,
    },
    {
      label: 'Fastest',
      value: records.highest_wpm_value != null ? `${Math.round(records.highest_wpm_value)} wpm` : '—',
      item: records.highest_wpm,
    },
  ];
  return (
    <div className="grid grid-cols-3 gap-[14px]">
      {tiles.map((r) => (
        <Panel key={r.label} className="px-[22px] py-5 min-w-0">
          <div className="flex items-center gap-2">
            <TrophyIcon />
            <span className="eyebrow">{r.label}</span>
          </div>
          <div className="num text-[30px] font-semibold tracking-[-1.2px] mt-[10px] text-text-primary truncate">
            {r.value}
          </div>
          <p className="text-[12px] leading-[1.5] text-text-secondary mt-2 truncate">
            {r.item ? `“${r.item.text}”` : 'Your best will land here.'}
          </p>
          <div className="text-[11px] text-text-tertiary mt-1 min-h-[16px] truncate">
            {r.item
              ? [formatRecordDate(r.item.created_at), r.item.target_app ? formatAppName(r.item.target_app) : null]
                  .filter(Boolean)
                  .join(' · ')
              : ''}
          </div>
        </Panel>
      ))}
    </div>
  );
}

// ---------- Words + fillers ----------

function MostUsedWords({ words }: { words: Array<{ word: string; count: number }> }) {
  return (
    <Panel className="px-6 py-[22px]">
      <PanelTitle title="Words you use most" sub='Common words like "the" are left out' />
      {words.length === 0 ? (
        <p className="text-[12px] text-text-tertiary mt-4">
          A few dictations in and your most-used words will show up here.
        </p>
      ) : (
        <div className="flex flex-wrap gap-2 mt-4">
          {words.map((w, i) => (
            <span
              key={w.word}
              className="inline-flex items-center gap-2 px-3 py-[6px] rounded-full bg-bg-control border border-border-hairline text-[13px]"
              style={{ color: i < 3 ? 'var(--text-primary)' : 'var(--text-secondary)' }}
            >
              {w.word}
              <span className="num text-[11px] text-text-tertiary">{w.count}</span>
            </span>
          ))}
        </div>
      )}
    </Panel>
  );
}

function FillerWordsCard({
  fillers,
  totalRemoved,
  progress,
}: {
  fillers: Array<{ word: string; count: number }>;
  totalRemoved: number;
  progress: FillerProgress | null;
}) {
  const top = fillers[0]?.count ?? 1;
  return (
    <Panel className="px-6 py-[22px]">
      <PanelTitle
        title="Filler words removed"
        sub="Cleaned out before pasting"
        right={
          <span className="num text-[28px] font-semibold tracking-[-1px] leading-none text-text-primary">
            {totalRemoved.toLocaleString()}
          </span>
        }
      />
      {fillers.length === 0 ? (
        <p className="text-[12px] text-text-tertiary mt-4 leading-[1.55]">
          Nothing removed yet. Once Murmr trims your first "um" or "uh", the top ones show up here.
        </p>
      ) : (
        <div className="flex flex-col gap-[10px] mt-4">
          {fillers.slice(0, 5).map((f) => (
            <div
              key={f.word}
              className="grid items-center gap-[10px] text-[12px]"
              style={{ gridTemplateColumns: '64px 1fr 40px' }}
            >
              <span className="text-text-secondary truncate">{f.word}</span>
              <span className="h-2 rounded-[4px] bg-bg-control overflow-hidden">
                <span
                  className="block h-full rounded-[4px]"
                  style={{ width: `${Math.max(4, (f.count / Math.max(top, 1)) * 100)}%`, background: 'var(--bar-neutral)' }}
                />
              </span>
              <span className="num text-text-tertiary text-right">{f.count.toLocaleString()}</span>
            </div>
          ))}
        </div>
      )}
      <FillerProgressLine progress={progress} />
    </Panel>
  );
}

function FillerProgressLine({ progress }: { progress: FillerProgress | null }) {
  if (!progress) return null;
  const delta =
    progress.prior_count > 0
      ? ((progress.current_count - progress.prior_count) / progress.prior_count) * 100
      : null;
  let text: string;
  let down = false;
  if (delta === null) {
    text = `"${progress.word}": ${progress.current_count.toLocaleString()} in the last ${progress.window_days} days.`;
  } else if (delta < -1) {
    down = true;
    text = `You say "${progress.word}" ${Math.abs(delta).toFixed(0)}% less than the ${progress.window_days} days before.`;
  } else if (delta > 5) {
    text = `You say "${progress.word}" ${delta.toFixed(0)}% more than the ${progress.window_days} days before.`;
  } else {
    text = `You say "${progress.word}" about as often as the ${progress.window_days} days before.`;
  }
  return (
    <div className="flex items-center gap-2 mt-4 pt-[14px] border-t border-border-hairline text-[12px] text-text-secondary">
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke={down ? 'var(--accent)' : 'var(--text-tertiary)'}
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
        style={{ transform: down || delta === null ? undefined : delta > 5 ? 'rotate(180deg)' : 'rotate(-90deg)' }}
      >
        <path d="M12 5v14M5 12l7 7 7-7" />
      </svg>
      {text}
    </div>
  );
}

// ---------- Speaking habits ----------

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
    <MilestoneShell title="Phrases you say a lot" unlocked={unlocked} unlockAt={unlockAt} totalSoFar={totalSoFar}>
      {phrases.length === 0 ? (
        <p className="text-[12px] text-text-tertiary leading-[1.55] mt-3">
          Once you've used a few short phrases more than once, they'll show up here.
        </p>
      ) : (
        <div className="flex flex-col mt-3">
          {phrases.slice(0, 6).map((p) => (
            <div
              key={p.phrase}
              className="flex justify-between gap-3 py-[10px] border-b border-border-hairline last:border-b-0 text-[13px]"
            >
              <span className="text-text-primary truncate">“{p.phrase}”</span>
              <span className="num text-text-tertiary">{p.count}×</span>
            </div>
          ))}
        </div>
      )}
    </MilestoneShell>
  );
}

type Theme = UsageSummary['themes'][number];

function PersonaCard({
  themes,
  totalTranscriptions,
  avgWords,
  unlockAt,
  unlocked,
  totalSoFar,
}: {
  themes: Theme[];
  totalTranscriptions: number;
  avgWords: number;
  unlockAt: number;
  unlocked: boolean;
  totalSoFar: number;
}) {
  return (
    <MilestoneShell title="What you talk about" unlocked={unlocked} unlockAt={unlockAt} totalSoFar={totalSoFar}>
      {themes.length === 0 ? (
        <p className="text-[12px] text-text-tertiary leading-[1.55] mt-3">
          Not enough yet. Once you've dictated about a few different things, your main themes show
          up here.
        </p>
      ) : (
        <Persona themes={themes} totalTranscriptions={totalTranscriptions} avgWords={avgWords} />
      )}
    </MilestoneShell>
  );
}

// Theme id → a short archetype so the blurb feels personal rather than a list.
const PROFILE: Record<string, { archetype: string; verb: string }> = {
  building: { archetype: 'a builder', verb: "you're shipping things" },
  coordinating: { archetype: 'a connector', verb: "you're aligning people" },
  planning: { archetype: 'a planner', verb: "you're mapping out what's next" },
  personal: { archetype: 'a homebody', verb: 'your life is the throughline' },
  writing: { archetype: 'a writer', verb: 'you think on the page' },
  money: { archetype: 'an operator', verb: 'you keep the books in mind' },
  travel: { archetype: 'on the move', verb: "you're rarely in one place" },
  leisure: { archetype: 'a curator', verb: 'culture is part of the rhythm' },
  health: { archetype: 'an athlete', verb: "you're tracking the body" },
  errands: { archetype: 'an organizer', verb: 'small tasks add up' },
};

function Persona({
  themes,
  totalTranscriptions,
  avgWords,
}: {
  themes: Theme[];
  totalTranscriptions: number;
  avgWords: number;
}) {
  const top = themes[0];
  const profile = PROFILE[top.theme] ?? { archetype: 'a thinker', verb: 'you cover broad ground' };
  const totalCovered = themes.reduce((sum, t) => sum + t.transcription_count, 0);
  const topShare = totalCovered > 0 ? top.transcription_count / totalCovered : 0;
  const variety = topShare < 0.4 ? 'broad-ranging' : topShare < 0.6 ? 'with a clear lean' : 'pretty focused';
  const style =
    avgWords < 12 ? 'short and to the point' : avgWords < 30 ? 'a comfortable middle length' : 'longer, thinking out loud';

  return (
    <div className="mt-3">
      <p className="text-[13px] text-text-secondary leading-[1.55] m-0">
        You're <span className="text-text-primary font-medium">{profile.archetype}</span>: {profile.verb}. Your
        dictations are {style}, {variety}.
      </p>
      <div className="flex flex-col gap-[14px] mt-4">
        {themes.slice(0, 4).map((t) => {
          const share = totalTranscriptions > 0 ? Math.round((t.transcription_count / totalTranscriptions) * 100) : 0;
          return (
            <div key={t.theme} className="min-w-0">
              <div className="flex justify-between text-[13px]">
                <span className="text-text-primary font-medium truncate">{t.label}</span>
                <span className="num text-text-tertiary">
                  {share}% · {t.transcription_count.toLocaleString()}
                </span>
              </div>
              <div className="text-[12px] text-text-tertiary mt-[3px] truncate">
                {t.sample_words.length > 0 ? t.sample_words.join(', ') : 'No samples yet'}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------- Icons ----------

function TrophyIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="var(--accent)"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8 21h8M12 17v4M7 4h10v5a5 5 0 0 1-10 0z" />
      <path d="M17 5h3v2a3 3 0 0 1-3 3M7 5H4v2a3 3 0 0 0 3 3" />
    </svg>
  );
}

function LockIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" aria-hidden="true">
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
  );
}

// ---------- Heatmap helpers ----------

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

interface HeatCell {
  bucket: number;
  count: number;
  dayInt: number;
  isToday: boolean;
  future: boolean;
}

function buildHeatmapGrid(days: Array<{ day: number; count: number }>): HeatCell[] {
  // Map yyyymmdd → count for fast lookup.
  const counts = new Map<number, number>();
  for (const d of days) counts.set(d.day, d.count);

  // Today's date in yyyymmdd (local, 4am-shifted to match the backend).
  const todayInt = todayWith4amOffset();
  // Columns are weeks (Mon–Sun, top to bottom), oldest on the left. The last
  // column holds the current week, so days after today are left blank.
  const todayRow = (weekdayOf(todayInt) + 6) % 7; // Monday = 0
  const totalCells = HEATMAP_WEEKS * 7;
  const lastCellOffset = 6 - todayRow; // cells after today in the final column

  let maxCount = 0;
  for (const c of counts.values()) if (c > maxCount) maxCount = c;

  const cells: HeatCell[] = [];
  for (let i = 0; i < totalCells; i++) {
    const offsetFromToday = totalCells - 1 - lastCellOffset - i;
    const future = offsetFromToday < 0;
    const dayInt = future ? addDays(todayInt, -offsetFromToday) : subtractDays(todayInt, offsetFromToday);
    const count = future ? 0 : counts.get(dayInt) ?? 0;
    cells.push({ bucket: bucketFor(count, maxCount), count, dayInt, isToday: offsetFromToday === 0, future });
  }
  return cells;
}

function bucketFor(count: number, max: number): number {
  if (count <= 0) return 0;
  if (max <= 1) return 4;
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

function dateOf(yyyymmdd: number): Date {
  const y = Math.floor(yyyymmdd / 10000);
  const m = Math.floor((yyyymmdd / 100) % 100) - 1;
  const d = yyyymmdd % 100;
  return new Date(y, m, d, 12, 0, 0); // local midday avoids DST jitter
}

function toInt(dt: Date): number {
  return dt.getFullYear() * 10000 + (dt.getMonth() + 1) * 100 + dt.getDate();
}

function subtractDays(yyyymmdd: number, n: number): number {
  const dt = dateOf(yyyymmdd);
  dt.setDate(dt.getDate() - n);
  return toInt(dt);
}

function addDays(yyyymmdd: number, n: number): number {
  return subtractDays(yyyymmdd, -n);
}

function weekdayOf(yyyymmdd: number): number {
  return dateOf(yyyymmdd).getDay();
}

function formatDayLong(yyyymmdd: number): string {
  return dateOf(yyyymmdd).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
}

// ---------- Formatting ----------

function formatWeek(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function formatHourLong(h: number): string {
  if (h === 0) return 'midnight';
  if (h === 12) return 'noon';
  if (h < 12) return `${h} am`;
  return `${h - 12} pm`;
}

function formatHourRange(h: number): string {
  const f = (x: number) => (x % 24 === 0 ? '12am' : x === 12 ? '12pm' : x < 12 ? `${x}am` : `${x - 12}pm`);
  return `${f(h)}–${f((h + 1) % 24)}`;
}

function formatRecordDate(unix_ms: number): string {
  const d = new Date(unix_ms);
  const days = Math.floor((Date.now() - d.getTime()) / 86_400_000);
  if (days < 1) return 'Today';
  if (days < 2) return 'Yesterday';
  if (days < 7) return `${days} days ago`;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function formatRecordDuration(ms: number): string {
  const totalSec = Math.round(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  if (m === 0) return `${s}s`;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function formatAppName(app: string): string {
  if (app === '(unknown)') return 'Unknown app';
  return app.replace(/\.(exe|app)$/i, '');
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
  if (words < 200) return 'About a long email';
  if (words < 1_000) return 'About 3 short emails';
  if (words < 5_000) return 'About 10 short emails';
  if (words < 20_000) return 'About a short story';
  if (words < 100_000) return 'About a novella';
  if (words < 180_000) return "About a novel's worth";
  return `About ${Math.floor(words / 90_000)} novels' worth`;
}
