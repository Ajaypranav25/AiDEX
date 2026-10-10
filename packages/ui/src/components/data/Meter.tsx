import { useId, type ComponentPropsWithRef, type ReactNode } from "react";
import { AlertTriangleIcon } from "../../icons/interface";
import { cx } from "../../utils/cx";
import { toNonNegative, toRatio } from "./chart";
import styles from "./Meter.module.css";

export type MeterTone = "accent" | "success" | "warning" | "danger";

export type MeterProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
  /** Visible name, also the meter's accessible name, e.g. "Drive storage". */
  label: ReactNode;
  value: number;
  max?: number;
  /** Human-readable reading, e.g. "312 GB of 1 TB"; defaults to a percentage. */
  valueText?: string;
  /** Fixed tone. Without it the tone follows `warnAt` / `dangerAt`, for meters where high is bad. */
  tone?: MeterTone;
  /** Fraction of `max` from which the meter shows warning (default 0.75). */
  warnAt?: number;
  /** Fraction of `max` from which the meter shows danger (default 0.9). */
  dangerAt?: number;
};

function toneFor(ratio: number, warnAt: number, dangerAt: number): MeterTone {
  if (ratio >= dangerAt) return "danger";
  if (ratio >= warnAt) return "warning";
  return "accent";
}

/** Labelled usage bar for a single value against a limit, such as disk or memory use. */
export function Meter({
  label,
  value,
  max = 100,
  valueText,
  tone,
  warnAt = 0.75,
  dangerAt = 0.9,
  className,
  ...props
}: MeterProps) {
  const labelId = useId();
  const ratio = toRatio(value, max);
  const safeMax = Number.isFinite(max) && max > 0 ? max : 1;
  const resolvedTone = tone ?? toneFor(ratio, warnAt, dangerAt);
  const reading = valueText ?? `${Math.round(ratio * 100)}%`;
  const elevated = resolvedTone === "warning" || resolvedTone === "danger";

  // role="meter" makes its children presentational, so the label and reading are announced once,
  // as the meter's name and value text.
  return (
    <div
      role="meter"
      aria-labelledby={labelId}
      aria-valuemin={0}
      aria-valuemax={safeMax}
      aria-valuenow={Math.min(toNonNegative(value), safeMax)}
      aria-valuetext={reading}
      className={cx(styles.meter, styles[resolvedTone], className)}
      {...props}
    >
      <div className={styles.header}>
        <span id={labelId} className={styles.label}>
          {label}
        </span>
        <span className={styles.reading}>
          {/* The icon is a shape cue so an elevated reading never relies on color alone. */}
          {elevated ? <AlertTriangleIcon size={14} className={styles.alert} /> : null}
          {reading}
        </span>
      </div>
      <div className={styles.track}>
        <div className={styles.fill} style={{ width: `${ratio * 100}%` }} />
      </div>
    </div>
  );
}
