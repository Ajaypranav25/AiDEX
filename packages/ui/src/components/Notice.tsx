import type { ComponentPropsWithRef, ReactNode } from "react";
import { AlertTriangleIcon, CheckCircleIcon, InfoIcon, XCircleIcon } from "../icons";
import { cx } from "../utils/cx";
import styles from "./Notice.module.css";

export type NoticeProps = ComponentPropsWithRef<"div"> & {
  tone?: "info" | "success" | "warning" | "danger";
  /** `plain` is an inline note with an icon; `banner` adds a tinted, bordered box for outcomes. */
  variant?: "plain" | "banner";
  /** Trailing control that resolves the notice, typically a `Button` such as "Try again". */
  action?: ReactNode;
};

const ICON_BY_TONE = { info: InfoIcon, success: CheckCircleIcon, warning: AlertTriangleIcon, danger: XCircleIcon } as const;

/** Short contextual message with a tone icon. Pass `role="alert"` for errors that appear after an action. */
export function Notice({ tone = "info", variant = "plain", action, className, children, ...props }: NoticeProps) {
  const Icon = ICON_BY_TONE[tone];
  return (
    <div className={cx(styles.notice, styles[tone], styles[variant], className)} {...props}>
      <Icon className={styles.icon} />
      <div className={styles.body}>{children}</div>
      {action ? <div className={styles.action}>{action}</div> : null}
    </div>
  );
}
