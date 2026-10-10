import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./Button.module.css";

export type ButtonVariant = "raised" | "outline" | "ghost";
export type ButtonSize = "md" | "lg";

export type ButtonProps = ComponentPropsWithRef<"button"> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Leading icon; sized by the button. */
  icon?: ReactNode;
  /** Trailing content such as a keyboard hint, pushed to the end. */
  trailing?: ReactNode;
  fullWidth?: boolean;
};

export function Button({
  variant = "outline",
  size = "md",
  icon,
  trailing,
  fullWidth = false,
  type = "button",
  className,
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(styles.button, styles[variant], styles[size], fullWidth && styles.fullWidth, className)}
      {...props}
    >
      {icon ? <span className={styles.icon}>{icon}</span> : null}
      <span className={styles.label}>{children}</span>
      {trailing ? <span className={styles.trailing}>{trailing}</span> : null}
    </button>
  );
}
