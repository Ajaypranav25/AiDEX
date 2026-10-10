import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import type { SpaceStep } from "./Stack";
import gapStyles from "./gap.module.css";
import styles from "./Grid.module.css";

/** `ref`, `className`, and other div props apply to the inner grid element. */
export type GridProps = ComponentPropsWithRef<"div"> & {
  /** Column count at full width; collapses to two, then one, as the grid narrows. */
  columns?: 2 | 3 | 4;
  gap?: SpaceStep;
};

/** Equal-width card grid that responds to its own width rather than the viewport. */
export function Grid({ columns = 3, gap = 4, className, ...props }: GridProps) {
  return (
    <div className={styles.container}>
      <div className={cx(styles.grid, styles[`columns${columns}`], gapStyles[`gap${gap}`], className)} {...props} />
    </div>
  );
}
