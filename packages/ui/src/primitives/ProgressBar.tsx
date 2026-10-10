import { cx } from "../utils/cx";
import styles from "./ProgressBar.module.css";

export type ProgressBarProps = {
  /** Accessible name, e.g. "Detection progress". */
  label: string;
  value: number;
  max?: number;
  /** Shows "value / max" beside the bar. */
  showCount?: boolean;
  className?: string;
};

export function ProgressBar({ label, value, max = 100, showCount = false, className }: ProgressBarProps) {
  const safeMax = Number.isFinite(max) && max > 0 ? max : 1;
  const clamped = Number.isFinite(value) ? Math.min(Math.max(value, 0), safeMax) : 0;
  return (
    <div className={cx(styles.progress, className)}>
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={safeMax}
        aria-valuenow={clamped}
        aria-valuetext={`${clamped} of ${safeMax}`}
        className={styles.track}
      >
        <div className={styles.fill} style={{ width: `${(clamped / safeMax) * 100}%` }} />
      </div>
      {showCount ? (
        <span className={styles.count} aria-hidden="true">
          {clamped} / {safeMax}
        </span>
      ) : null}
    </div>
  );
}
