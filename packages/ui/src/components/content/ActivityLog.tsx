import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import a11y from "./a11y.module.css";
import styles from "./ActivityLog.module.css";

export type ActivityTone = "default" | "success" | "danger" | "accent";

export type ActivityEntry = {
  /** Stable key; falls back to the entry's position, which is fine for append-only logs. */
  id?: string;
  time: string;
  text: ReactNode;
  tone?: ActivityTone;
};

export type ActivityLogProps = Omit<ComponentPropsWithRef<"ol">, "children"> & {
  entries: readonly ActivityEntry[];
  /** Announces newly appended entries to screen readers; use for logs that stream in. */
  live?: boolean;
};

// Outcome tones get a spoken prefix so success or failure is not conveyed by color alone.
const SPOKEN_TONE: Partial<Record<ActivityTone, string>> = {
  success: "Success: ",
  danger: "Error: ",
};

/** Monospace, time-stamped event log. */
export function ActivityLog({ entries, live = false, className, ...props }: ActivityLogProps) {
  return (
    <ol className={cx(styles.log, className)} aria-live={live ? "polite" : undefined} {...props}>
      {entries.map((entry, index) => {
        const tone = entry.tone ?? "default";
        const spoken = SPOKEN_TONE[tone];
        return (
          <li key={entry.id ?? index} className={cx(styles.entry, styles[tone])}>
            <span className={styles.time}>{entry.time}</span>
            <span className={styles.text}>
              {spoken ? <span className={a11y.visuallyHidden}>{spoken}</span> : null}
              {entry.text}
            </span>
          </li>
        );
      })}
    </ol>
  );
}
