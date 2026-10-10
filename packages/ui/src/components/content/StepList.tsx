import type { ComponentPropsWithRef, ReactNode } from "react";
import { CheckIcon, XIcon } from "../../icons";
import { cx } from "../../utils/cx";
import a11y from "./a11y.module.css";
import styles from "./StepList.module.css";

export type StepStatus = "done" | "active" | "pending" | "failed";

export type Step = {
  title: string;
  description?: string;
  status: StepStatus;
};

export type StepListProps = Omit<ComponentPropsWithRef<"ol">, "children"> & {
  steps: readonly Step[];
};

const STATUS_TEXT: Record<StepStatus, string> = {
  done: "done",
  active: "in progress",
  pending: "pending",
  failed: "failed",
};

function markerContent(status: StepStatus, number: number): ReactNode {
  if (status === "done") return <CheckIcon strokeWidth={2.25} />;
  if (status === "failed") return <XIcon strokeWidth={2.25} />;
  return number;
}

/** Ordered plan steps; each marker shows the step number, a check when done, or an X when failed. */
export function StepList({ steps, className, ...props }: StepListProps) {
  return (
    <ol className={cx(styles.list, className)} {...props}>
      {steps.map((step, index) => (
        <li
          key={index}
          className={cx(styles.step, styles[step.status])}
          aria-current={step.status === "active" ? "step" : undefined}
        >
          <span className={styles.marker} aria-hidden="true">
            {markerContent(step.status, index + 1)}
          </span>
          <div className={styles.text}>
            <p className={styles.title}>
              {step.title}
              <span className={a11y.visuallyHidden}> ({STATUS_TEXT[step.status]})</span>
            </p>
            {step.description ? <p className={styles.description}>{step.description}</p> : null}
          </div>
        </li>
      ))}
    </ol>
  );
}
