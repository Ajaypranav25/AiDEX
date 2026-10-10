import type { ComponentPropsWithRef, ReactNode } from "react";
import { CheckIcon } from "../icons/interface";
import { cx } from "../utils/cx";
import styles from "./Checkbox.module.css";

export type CheckboxProps = Omit<ComponentPropsWithRef<"input">, "type" | "children"> & {
  label: ReactNode;
  description?: ReactNode;
  /** Applied to the outer label, not the input. */
  className?: string;
};

/** Native checkbox with a custom box, a label, and optional description. */
export function Checkbox({ label, description, className, ...inputProps }: CheckboxProps) {
  return (
    <label className={cx(styles.checkbox, inputProps.disabled && styles.disabled, className)}>
      <span className={styles.control}>
        <input {...inputProps} type="checkbox" className={styles.input} />
        <CheckIcon className={styles.mark} strokeWidth={2.5} />
      </span>
      <span className={styles.text}>
        <span className={styles.label}>{label}</span>
        {description ? <span className={styles.description}>{description}</span> : null}
      </span>
    </label>
  );
}
