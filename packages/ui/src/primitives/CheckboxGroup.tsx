import type { ReactNode } from "react";
import { cx } from "../utils/cx";
import { Checkbox } from "./Checkbox";
import fieldStyles from "./field.module.css";
import styles from "./CheckboxGroup.module.css";

export type CheckboxOption = {
  value: string;
  title: string;
  description?: ReactNode;
  /** Trailing detail such as a size or "Recommended". */
  meta?: ReactNode;
  disabled?: boolean;
};

export type CheckboxGroupProps = {
  /** Group legend. */
  label: string;
  labelVisibility?: "visible" | "hidden";
  options: readonly CheckboxOption[];
  /** Values of the checked options. */
  value: readonly string[];
  onValueChange: (value: string[]) => void;
  className?: string;
};

/** Multiple choice as a ruled list of checkboxes, matching `ChoiceCardGroup variant="rows"`. */
export function CheckboxGroup({ label, labelVisibility = "visible", options, value, onValueChange, className }: CheckboxGroupProps) {
  const toggle = (optionValue: string, checked: boolean) =>
    onValueChange(checked ? [...value, optionValue] : value.filter((item) => item !== optionValue));

  return (
    <fieldset className={cx(styles.group, className)}>
      <legend className={labelVisibility === "visible" ? cx(fieldStyles.label, styles.legend) : fieldStyles.visuallyHidden}>
        {label}
      </legend>
      <div className={styles.rows}>
        {options.map((option) => (
          <div key={option.value} className={styles.row}>
            <Checkbox
              className={styles.checkbox}
              label={option.title}
              description={option.description}
              checked={value.includes(option.value)}
              disabled={option.disabled}
              onChange={(event) => toggle(option.value, event.target.checked)}
            />
            {option.meta ? <span className={styles.meta}>{option.meta}</span> : null}
          </div>
        ))}
      </div>
    </fieldset>
  );
}
