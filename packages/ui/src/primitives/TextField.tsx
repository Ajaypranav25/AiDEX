import { useId, type ComponentPropsWithRef, type ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./TextField.module.css";

export type TextFieldProps = Omit<ComponentPropsWithRef<"input">, "size"> & {
  /** Accessible label; visually hidden, so surrounding content must make the field's purpose clear. */
  label: string;
  leadingIcon?: ReactNode;
  /** Trailing control such as an `IconButton`; kept outside the label so it stays independently clickable. */
  trailing?: ReactNode;
  shape?: "rounded" | "pill";
  /** Validation message shown under the field, announced when it appears and linked to the input. */
  error?: string;
  /** Applied to the outer wrapper, not the input. */
  className?: string;
};

export function TextField({
  label,
  leadingIcon,
  trailing,
  shape = "rounded",
  error,
  className,
  "aria-describedby": describedBy,
  ...inputProps
}: TextFieldProps) {
  const errorId = useId();
  const describedByIds = [describedBy, error ? errorId : undefined].filter(Boolean).join(" ") || undefined;
  return (
    <div className={cx(styles.root, className)}>
      <div className={cx(styles.field, styles[shape], error && styles.invalid)}>
        <label className={styles.control}>
          {leadingIcon ? <span className={styles.icon}>{leadingIcon}</span> : null}
          <span className={styles.visuallyHidden}>{label}</span>
          <input
            {...inputProps}
            className={styles.input}
            aria-invalid={error ? true : inputProps["aria-invalid"]}
            aria-describedby={describedByIds}
          />
        </label>
        {trailing ? <span className={styles.trailing}>{trailing}</span> : null}
      </div>
      {error ? (
        <p id={errorId} role="alert" className={styles.error}>
          {error}
        </p>
      ) : null}
    </div>
  );
}
