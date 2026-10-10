import { useId, type ReactNode } from "react";
import { cx } from "../utils/cx";
import fieldStyles from "./field.module.css";
import styles from "./ChoiceCardGroup.module.css";

export type ChoiceCardOption<Value extends string> = {
  value: Value;
  title: string;
  description?: ReactNode;
  icon?: ReactNode;
  /** Trailing detail such as a size estimate or a badge. */
  meta?: ReactNode;
  disabled?: boolean;
};

export type ChoiceCardGroupProps<Value extends string> = {
  /** Group legend. */
  label: string;
  labelVisibility?: "visible" | "hidden";
  options: readonly ChoiceCardOption<Value>[];
  value: Value;
  onValueChange: (value: Value) => void;
  columns?: 1 | 2 | 3;
  className?: string;
};

/** Single choice presented as cards; built on native radios so arrow keys move the selection. */
export function ChoiceCardGroup<Value extends string>({
  label,
  labelVisibility = "visible",
  options,
  value,
  onValueChange,
  columns = 1,
  className,
}: ChoiceCardGroupProps<Value>) {
  const name = useId();
  return (
    <fieldset className={cx(styles.group, className)}>
      <legend className={labelVisibility === "visible" ? cx(fieldStyles.label, styles.legend) : fieldStyles.visuallyHidden}>
        {label}
      </legend>
      <div className={cx(styles.options, styles[`columns${columns}`])}>
        {options.map((option) => {
          const checked = option.value === value;
          return (
            <label key={option.value} className={cx(styles.card, checked && styles.checked, option.disabled && styles.disabled)}>
              <input
                type="radio"
                name={name}
                value={option.value}
                checked={checked}
                disabled={option.disabled}
                className={styles.radio}
                onChange={() => onValueChange(option.value)}
              />
              {option.icon ? <span className={styles.icon}>{option.icon}</span> : null}
              <span className={styles.text}>
                <span className={styles.title}>{option.title}</span>
                {option.description ? <span className={styles.description}>{option.description}</span> : null}
              </span>
              {option.meta ? <span className={styles.meta}>{option.meta}</span> : null}
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
