import type { ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./PageFrame.module.css";

export type PageFrameProps = {
  /** `narrow` for forms and lists, `default` for most pages, `wide` for dashboards and graphs. */
  width?: "narrow" | "default" | "wide";
  children: ReactNode;
  className?: string;
};

/** Scrollable page body inside `AppShell`: centered, width-capped, with page padding and section rhythm. */
export function PageFrame({ width = "default", children, className }: PageFrameProps) {
  return <div className={cx(styles.frame, styles[width], className)}>{children}</div>;
}
