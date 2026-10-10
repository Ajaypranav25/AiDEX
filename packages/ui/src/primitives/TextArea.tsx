import { useId, type ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import fieldStyles from "./field.module.css";
import styles from "./TextArea.module.css";

export type TextAreaProps = ComponentPropsWithRef<"textarea"> & {
  label: string;
  labelVisibility?: "visible" | "hidden";
  hint?: string;
  error?: string;
  /** Monospace text, for prompts, configuration, and skill definitions. */
  monospace?: boolean;
  /** Applied to the outer wrapper, not the textarea. */
  className?: string;
};

/** Multi-line text field with label, hint, and error, styled to the field system. */
export function TextArea({
  label,
  labelVisibility = "visible",
  hint,
  error,
  monospace = false,
  className,
  id,
  rows = 5,
  ...textareaProps
}: TextAreaProps) {
  const generatedId = useId();
  const textareaId = id ?? generatedId;
  const hintId = `${textareaId}-hint`;
  const errorId = `${textareaId}-error`;
  const describedBy = [hint ? hintId : undefined, error ? errorId : undefined].filter(Boolean).join(" ") || undefined;

  return (
    <div className={cx(fieldStyles.root, className)}>
      <label className={labelVisibility === "visible" ? fieldStyles.label : fieldStyles.visuallyHidden} htmlFor={textareaId}>
        {label}
      </label>
      <textarea
        {...textareaProps}
        id={textareaId}
        rows={rows}
        className={cx(styles.textarea, monospace && styles.monospace, error && styles.invalid)}
        aria-invalid={error ? true : textareaProps["aria-invalid"]}
        aria-describedby={describedBy}
      />
      {hint ? (
        <p id={hintId} className={fieldStyles.hint}>
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} role="alert" className={fieldStyles.error}>
          {error}
        </p>
      ) : null}
    </div>
  );
}
