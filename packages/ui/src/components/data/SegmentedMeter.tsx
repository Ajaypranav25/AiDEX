import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import { CHART_COLOR_COUNT, formatNumber, toNonNegative, type ChartColor } from "./chart";
import styles from "./SegmentedMeter.module.css";

export type MeterSegment = {
  label: string;
  value: number;
  /** Chart slot; defaults to the segment's position so colors follow a fixed order. */
  color?: ChartColor;
};

export type SegmentedMeterProps = Omit<ComponentPropsWithRef<"figure">, "children"> & {
  /** Visible name, also the figure's accessible name, e.g. "Drive storage". */
  label: ReactNode;
  segments: readonly MeterSegment[];
  /** Capacity the segments fill; space past their total shows as the unfilled track. */
  max: number;
  /** Summary beside the label, e.g. "640 GB of 1 TB used". */
  valueText?: ReactNode;
  /** Adds a legend entry for the unfilled track, e.g. "Free". */
  remainderLabel?: string;
  /** Formats legend values; defaults to a grouped number. */
  formatValue?: (value: number) => string;
};

type Part = { label: string; value: number; color: ChartColor };

// Colors are never cycled: past the last slot, the tail folds into "Other".
function toParts(segments: readonly MeterSegment[]): Part[] {
  const parts = segments.map((segment, index) => ({
    label: segment.label,
    value: toNonNegative(segment.value),
    color: segment.color ?? (((index % CHART_COLOR_COUNT) + 1) as ChartColor),
  }));
  if (parts.length <= CHART_COLOR_COUNT) return parts;
  const head = parts.slice(0, CHART_COLOR_COUNT - 1);
  const rest = parts.slice(CHART_COLOR_COUNT - 1).reduce((sum, part) => sum + part.value, 0);
  return [...head, { label: "Other", value: rest, color: CHART_COLOR_COUNT }];
}

/** Stacked bar showing how a capacity breaks down, with a text legend of every segment. */
export function SegmentedMeter({
  label,
  segments,
  max,
  valueText,
  remainderLabel,
  formatValue = (value) => formatNumber(value),
  className,
  ...props
}: SegmentedMeterProps) {
  const parts = toParts(segments);
  const total = parts.reduce((sum, part) => sum + part.value, 0);
  const remainder = Math.max(toNonNegative(max) - total, 0);

  return (
    <figure className={cx(styles.meter, className)} {...props}>
      <figcaption className={styles.header}>
        <span className={styles.label}>{label}</span>
        {valueText ? <span className={styles.summary}>{valueText}</span> : null}
      </figcaption>
      {/* The legend carries every value as text, so the bar itself is hidden from assistive technology. */}
      <div className={styles.track} aria-hidden="true">
        {parts.map((part, index) =>
          part.value > 0 ? (
            <span
              key={`${part.label}-${index}`}
              className={cx(styles.segment, styles[`color${part.color}`])}
              style={{ flexGrow: part.value }}
            />
          ) : null,
        )}
        {remainder > 0 ? <span className={styles.remainder} style={{ flexGrow: remainder }} /> : null}
      </div>
      <dl className={styles.legend}>
        {parts.map((part, index) => (
          <div key={`${part.label}-${index}`} className={styles.item}>
            <dt className={styles.itemLabel}>
              <span className={cx(styles.swatch, styles[`color${part.color}`])} aria-hidden="true" />
              {part.label}
            </dt>
            <dd className={styles.itemValue}>{formatValue(part.value)}</dd>
          </div>
        ))}
        {remainderLabel ? (
          <div className={styles.item}>
            <dt className={styles.itemLabel}>
              <span className={cx(styles.swatch, styles.remainder)} aria-hidden="true" />
              {remainderLabel}
            </dt>
            <dd className={styles.itemValue}>{formatValue(remainder)}</dd>
          </div>
        ) : null}
      </dl>
    </figure>
  );
}
