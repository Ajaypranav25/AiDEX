import type { ComponentPropsWithRef } from "react";
import { CheckIcon } from "../icons/interface";
import { cx } from "../utils/cx";
import styles from "./Stepper.module.css";

export type StepperStatus = "complete" | "current" | "upcoming";

export type StepperStep = {
  id: string;
  title: string;
  /** Short summary under the title, e.g. the current choice for a completed step. */
  description?: string;
  status: StepperStatus;
};

export type StepperProps = Omit<ComponentPropsWithRef<"nav">, "children" | "onSelect"> & {
  /** Accessible name of the navigation landmark, e.g. "Setup steps". */
  label: string;
  steps: readonly StepperStep[];
  /** Called when a completed step is chosen; current and upcoming steps are not interactive. */
  onSelect?: (id: string) => void;
};

const STATUS_TEXT: Record<StepperStatus, string> = {
  complete: "Complete",
  current: "Current step",
  upcoming: "Upcoming",
};

/** Vertical wizard progress: numbered steps with a visible text status; completed steps can be revisited. */
export function Stepper({ label, steps, onSelect, className, ...props }: StepperProps) {
  return (
    <nav aria-label={label} className={cx(styles.stepper, className)} {...props}>
      <ol className={styles.list}>
        {steps.map((step, index) => {
          const content = (
            <>
              <span className={styles.marker} aria-hidden="true">
                {step.status === "complete" ? <CheckIcon strokeWidth={2.25} /> : index + 1}
              </span>
              <span className={styles.text}>
                <span className={styles.title}>{step.title}</span>
                <span className={styles.status}>{STATUS_TEXT[step.status]}</span>
                {step.description ? <span className={styles.description}>{step.description}</span> : null}
              </span>
            </>
          );
          const interactive = step.status === "complete" && onSelect;
          return (
            <li
              key={step.id}
              className={cx(styles.step, styles[step.status])}
              aria-current={step.status === "current" ? "step" : undefined}
            >
              {interactive ? (
                <button type="button" className={cx(styles.row, styles.button)} onClick={() => onSelect(step.id)}>
                  {content}
                </button>
              ) : (
                <div className={styles.row}>{content}</div>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
