import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import styles from "./Switch.module.css";

export type SwitchProps = Omit<ComponentPropsWithRef<"button">, "role" | "onChange" | "children"> & {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  /** Accessible name when no visible `<label htmlFor>` points at this switch. */
  label?: string;
};

/** On/off toggle with switch semantics; changes apply immediately. */
export function Switch({ checked, onCheckedChange, label, className, type = "button", ...props }: SwitchProps) {
  return (
    <button
      type={type}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={cx(styles.switch, checked && styles.on, className)}
      onClick={() => onCheckedChange(!checked)}
      {...props}
    >
      <span className={styles.thumb} aria-hidden="true" />
    </button>
  );
}
