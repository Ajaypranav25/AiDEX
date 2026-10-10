import { CheckIcon } from "../icons/interface";
import { cx } from "../utils/cx";
import styles from "./StepTrack.module.css";

export type StepTrackStatus = "complete" | "current" | "upcoming";

export type StepTrackStep = {
  id: string;
  title: string;
  status: StepTrackStatus;
  /** Whether the step can be revisited; locked steps are shown but not clickable. */
  reachable?: boolean;
};

export type StepTrackProps = {
  /** Accessible name for the step navigation. */
  label: string;
  steps: readonly StepTrackStep[];
  onSelect?: (id: string) => void;
};

const STATUS_TEXT: Record<StepTrackStatus, string> = {
  complete: "completed",
  current: "current step",
  upcoming: "not started",
};

/** Horizontal numbered steps joined by a line; finished steps show a check, reachable ones are buttons. */
export function StepTrack({ label, steps, onSelect }: StepTrackProps) {
  return (
    <nav aria-label={label}>
      <ol className={styles.track} style={{ gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))` }}>
        {steps.map((step, index) => {
          const content = (
            <>
              <span className={styles.count} aria-hidden="true">
                {step.status === "complete" ? <CheckIcon strokeWidth={2.4} /> : index + 1}
              </span>
              <span className={styles.label}>
                {step.title}
                <span className={styles.visuallyHidden}>, {STATUS_TEXT[step.status]}</span>
              </span>
            </>
          );
          const interactive = Boolean(onSelect) && step.reachable && step.status !== "current";
          return (
            <li key={step.id} className={cx(styles.step, styles[step.status])}>
              {interactive ? (
                <button type="button" className={styles.target} onClick={() => onSelect?.(step.id)}>
                  {content}
                </button>
              ) : (
                <span className={styles.target} aria-current={step.status === "current" ? "step" : undefined}>
                  {content}
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
