import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./Badge.module.css";

export type BadgeProps = ComponentPropsWithRef<"span"> & {
  tone?: "accent" | "success" | "neutral";
  icon?: ReactNode;
};

/** Compact pill label for a status or category. */
export function Badge({ tone = "accent", icon, className, children, ...props }: BadgeProps) {
  return (
    <span className={cx(styles.badge, styles[tone], className)} {...props}>
      {icon ? <span className={styles.icon}>{icon}</span> : null}
      {children}
    </span>
  );
}
