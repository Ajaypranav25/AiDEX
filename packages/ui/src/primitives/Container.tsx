import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import styles from "./Container.module.css";

export type ContainerProps = ComponentPropsWithRef<"div"> & {
  /** Maximum content width. */
  size?: "sm" | "md" | "lg";
  /** Centers the container on the cross axis too when its parent is a flex column. */
  verticallyCentered?: boolean;
};

/** Horizontally centered, width-capped content region. */
export function Container({ size = "md", verticallyCentered = false, className, ...props }: ContainerProps) {
  return (
    <div
      className={cx(styles.container, styles[size], verticallyCentered && styles.verticallyCentered, className)}
      {...props}
    />
  );
}
