import { useId, type ComponentPropsWithRef } from "react";
import { ChevronDownIcon } from "../icons/interface";
import { cx } from "../utils/cx";
import fieldStyles from "./field.module.css";
import styles from "./Select.module.css";

export type SelectOption = { value: string; label: string; disabled?: boolean };

export type SelectProps = Omit<ComponentPropsWithRef<"select">, "children" | "size"> & {
  label: string;
  labelVisibility?: "visible" | "hidden";
  options: readonly SelectOption[];
  hint?: string;
  /** `field` matches text fields; `ghost` is a borderless inline picker (e.g. a model switcher in a toolbar). */
  variant?: "field" | "ghost";
  /** Applied to the outer wrapper, not the select. */
  className?: string;
};

/** Native select styled to the field system, so keyboard and screen-reader behavior stay platform-correct. */
export function Select({
  label,
  labelVisibility = "visible",
  options,
  hint,
  variant = "field",
  className,
  id,
  ...selectProps
}: SelectProps) {
  const generatedId = useId();
  const selectId = id ?? generatedId;
  const hintId = hint ? `${selectId}-hint` : undefined;
  const labelVisible = labelVisibility === "visible";

  return (
    <div className={cx(fieldStyles.root, variant === "ghost" && styles.inline, className)}>
      <label className={labelVisible ? fieldStyles.label : fieldStyles.visuallyHidden} htmlFor={selectId}>
        {label}
      </label>
      <span className={cx(styles.control, styles[variant])}>
        <select {...selectProps} id={selectId} className={styles.select} aria-describedby={hintId}>
          {options.map((option) => (
            <option key={option.value} value={option.value} disabled={option.disabled}>
              {option.label}
            </option>
          ))}
        </select>
        <ChevronDownIcon className={styles.chevron} />
      </span>
      {hint ? (
        <p id={hintId} className={fieldStyles.hint}>
          {hint}
        </p>
      ) : null}
    </div>
  );
}
