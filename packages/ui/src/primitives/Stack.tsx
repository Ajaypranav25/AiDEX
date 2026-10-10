import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import gapStyles from "./gap.module.css";
import styles from "./Stack.module.css";

/** Spacing step from the token scale; step N is N * 4px. */
export type SpaceStep = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12;

export type StackProps = ComponentPropsWithRef<"div"> & {
  direction?: "row" | "column";
  gap?: SpaceStep;
  align?: "start" | "center" | "end" | "stretch";
  justify?: "start" | "center" | "end" | "between";
  wrap?: boolean;
};

const ALIGN_CLASS = { start: "alignStart", center: "alignCenter", end: "alignEnd", stretch: "alignStretch" } as const;
const JUSTIFY_CLASS = { start: "justifyStart", center: "justifyCenter", end: "justifyEnd", between: "justifyBetween" } as const;

/** Flex layout primitive that spaces its children with token gaps. */
export function Stack({
  direction = "column",
  gap = 0,
  align = "stretch",
  justify = "start",
  wrap = false,
  className,
  ...props
}: StackProps) {
  return (
    <div
      className={cx(
        styles.stack,
        styles[direction],
        gapStyles[`gap${gap}`],
        styles[ALIGN_CLASS[align]],
        styles[JUSTIFY_CLASS[justify]],
        wrap && styles.wrap,
        className,
      )}
      {...props}
    />
  );
}
