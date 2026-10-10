import type { ComponentPropsWithRef } from "react";
import { AlertTriangleIcon, CheckCircleIcon, InfoIcon, XCircleIcon } from "../icons";
import { cx } from "../utils/cx";
import styles from "./Notice.module.css";

export type NoticeProps = ComponentPropsWithRef<"div"> & {
  tone?: "info" | "success" | "warning" | "danger";
  /** `plain` is an inline note with an icon; `banner` adds a tinted, bordered box for outcomes. */
  variant?: "plain" | "banner";
};

const ICON_BY_TONE = { info: InfoIcon, success: CheckCircleIcon, warning: AlertTriangleIcon, danger: XCircleIcon } as const;

/** Short contextual message with a tone icon. Pass `role="alert"` for errors that appear after an action. */
export function Notice({ tone = "info", variant = "plain", className, children, ...props }: NoticeProps) {
  const Icon = ICON_BY_TONE[tone];
  return (
    <div className={cx(styles.notice, styles[tone], styles[variant], className)} {...props}>
      <Icon className={styles.icon} />
      <div className={styles.body}>{children}</div>
    </div>
  );
}
