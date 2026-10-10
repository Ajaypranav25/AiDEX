import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import { formatNumber, toNonNegative, toRatio } from "./chart";
import styles from "./BarChart.module.css";

export type BarDatum = {
  label: string;
  value: number;
  /** Emphasizes this bar; the others recede to gray. */
  highlight?: boolean;
};

export type BarChartProps = Omit<ComponentPropsWithRef<"figure">, "children"> & {
  /** Names what is plotted, e.g. "Tokens per second by quantization"; also the accessible name. */
  caption: ReactNode;
  /** Visually hides the caption when a nearby heading already names the chart. */
  hideCaption?: boolean;
  data: readonly BarDatum[];
  /** Appended to each formatted value, e.g. "tok/s". */
  unit?: string;
  /** Scale maximum; defaults to the largest value. */
  max?: number;
  /** Formats each value label; overrides `unit`. */
  formatValue?: (value: number) => string;
};

/** Horizontal bar chart for comparing one measure across categories; every value is real text. */
export function BarChart({
  caption,
  hideCaption = false,
  data,
  unit,
  max,
  formatValue = (value) => formatNumber(value, unit),
  className,
  ...props
}: BarChartProps) {
  const scaleMax = max ?? Math.max(0, ...data.map((datum) => toNonNegative(datum.value)));
  const hasHighlight = data.some((datum) => datum.highlight);

  return (
    <figure className={cx(styles.chart, className)} {...props}>
      <figcaption className={hideCaption ? styles.visuallyHidden : styles.caption}>{caption}</figcaption>
      <dl className={styles.rows}>
        {data.map((datum, index) => {
          const muted = hasHighlight && !datum.highlight;
          return (
            <div
              key={`${datum.label}-${index}`}
              className={cx(styles.row, datum.highlight && styles.highlight, muted && styles.muted)}
            >
              <dt className={styles.label}>{datum.label}</dt>
              <dd className={styles.datum}>
                <span className={styles.track} aria-hidden="true">
                  <span className={styles.bar} style={{ width: `${toRatio(datum.value, scaleMax) * 100}%` }} />
                </span>
                <span className={styles.value}>{formatValue(datum.value)}</span>
              </dd>
            </div>
          );
        })}
      </dl>
    </figure>
  );
}
