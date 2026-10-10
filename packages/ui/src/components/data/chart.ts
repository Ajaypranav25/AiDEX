/** Categorical chart slot (`--aidex-chart-N`); assign in order, never cycle. */
export type ChartColor = 1 | 2 | 3 | 4 | 5 | 6;

export const CHART_COLOR_COUNT = 6;

// A fixed locale keeps server and client renders identical, avoiding hydration mismatches.
const numberFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 1 });

/** Formats a number with grouping and at most one decimal, plus an optional unit; non-finite values show a dash. */
export function formatNumber(value: number, unit?: string): string {
  if (!Number.isFinite(value)) return "—";
  const text = numberFormat.format(value);
  return unit ? `${text} ${unit}` : text;
}

/** Treats non-finite and negative values as zero. */
export function toNonNegative(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/** `value / max`, clamped to 0..1; 0 when `max` is not a positive number. */
export function toRatio(value: number, max: number): number {
  if (!Number.isFinite(max) || max <= 0) return 0;
  return Math.min(toNonNegative(value) / max, 1);
}
