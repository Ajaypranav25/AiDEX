import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./IconButton.module.css";

export type IconButtonVariant = "ghost" | "soft" | "solid" | "primary";
export type IconButtonSize = "sm" | "md" | "lg" | "xl";

export type IconButtonProps = Omit<ComponentPropsWithRef<"button">, "children"> & {
  /** Accessible name, also shown as the native tooltip. */
  label: string;
  icon: ReactNode;
  variant?: IconButtonVariant;
  size?: IconButtonSize;
  shape?: "rounded" | "circle";
  /** Highlights the button as the current selection, e.g. the active navigation destination. */
  selected?: boolean;
};

export function IconButton({
  label,
  icon,
  variant = "ghost",
  size = "md",
  shape = "rounded",
  selected = false,
  type = "button",
  className,
  ...props
}: IconButtonProps) {
  return (
    <button
      type={type}
      aria-label={label}
      title={label}
      className={cx(styles.button, styles[variant], styles[size], styles[shape], selected && styles.selected, className)}
      {...props}
    >
      {icon}
    </button>
  );
}
