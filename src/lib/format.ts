/// Formatting helpers shared across pages (date grouping, time, etc.).

const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
];

/** Group label for an entry's createdAt — Today / Yesterday / This week /
 *  This month / "Month YYYY". */
export function dateGroup(createdAtMs: number, now = Date.now()): string {
  const d = new Date(createdAtMs);
  const today = startOfDay(now);
  const yesterday = today - 86_400_000;
  const lastWeek = today - 7 * 86_400_000;
  const thisMonth = startOfMonth(now);
  const dStart = startOfDay(d.getTime());

  if (dStart === today) return 'Today';
  if (dStart === yesterday) return 'Yesterday';
  if (dStart > lastWeek) return 'Last week';
  if (dStart >= thisMonth) return 'This month';
  return `${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

export function formatTime(createdAtMs: number): string {
  const d = new Date(createdAtMs);
  let h = d.getHours();
  const m = d.getMinutes().toString().padStart(2, '0');
  const ampm = h >= 12 ? 'PM' : 'AM';
  h = h % 12 || 12;
  return `${h}:${m} ${ampm}`;
}

function startOfDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function startOfMonth(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), 1).getTime();
}
