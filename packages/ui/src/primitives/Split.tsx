import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import styles from "./Split.module.css";

export type SplitProps = ComponentPropsWithRef<"div"> & {
  /** Which side gets more width. */
  emphasis?: "equal" | "start" | "end";
  /** Draws a vertical rule between the two columns (horizontal when stacked). */
  divided?: boolean;
};

/** Two-column layout for paired sections (list and detail, plan and activity); stacks when narrow. */
export function Split({ emphasis = "equal", divided = false, className, ...props }: SplitProps) {
  return (
    <div className={styles.container}>
      <div className={cx(styles.split, styles[emphasis], divided && styles.divided, className)} {...props} />
    </div>
  );
}
