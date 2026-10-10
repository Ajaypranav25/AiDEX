import { useId } from "react";
import { CheckIcon } from "../icons/interface";
import { cx } from "../utils/cx";
import fieldStyles from "./field.module.css";
import styles from "./SwatchPicker.module.css";

export type Swatch = { value: string; label: string };

export type SwatchPickerProps = {
  label: string;
  labelVisibility?: "visible" | "hidden";
  /** Colors as CSS hex values. */
  swatches: readonly Swatch[];
  value: string;
  onValueChange: (value: string) => void;
  /** Adds a native color input for any custom color. */
  allowCustom?: boolean;
  customLabel?: string;
  className?: string;
};

/** Color choice from preset swatches, with an optional custom color. Swatches are radios named by `label`. */
export function SwatchPicker({
  label,
  labelVisibility = "hidden",
  swatches,
  value,
  onValueChange,
  allowCustom = false,
  customLabel = "Custom color",
  className,
}: SwatchPickerProps) {
  const name = useId();
  const isPreset = swatches.some((swatch) => swatch.value.toLowerCase() === value.toLowerCase());
  return (
    <fieldset className={cx(styles.picker, className)}>
      <legend className={labelVisibility === "visible" ? fieldStyles.label : fieldStyles.visuallyHidden}>{label}</legend>
      {swatches.map((swatch) => {
        const checked = swatch.value.toLowerCase() === value.toLowerCase();
        return (
          <label key={swatch.value} className={cx(styles.swatch, checked && styles.checked)} title={swatch.label}>
            <input
              type="radio"
              name={name}
              value={swatch.value}
              checked={checked}
              aria-label={swatch.label}
              className={styles.radio}
              onChange={() => onValueChange(swatch.value)}
            />
            {/* The swatch color is data, not theme, so it is set inline. */}
            <span className={styles.chip} style={{ background: swatch.value }} aria-hidden="true">
              {checked ? <CheckIcon className={styles.check} strokeWidth={2.5} /> : null}
            </span>
          </label>
        );
      })}
      {allowCustom ? (
        <label className={cx(styles.swatch, !isPreset && styles.checked)} title={customLabel}>
          <input
            type="color"
            value={value}
            aria-label={customLabel}
            className={styles.color}
            onChange={(event) => onValueChange(event.target.value)}
          />
        </label>
      ) : null}
    </fieldset>
  );
}
