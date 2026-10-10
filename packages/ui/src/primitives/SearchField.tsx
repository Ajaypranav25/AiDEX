import type { ComponentPropsWithRef } from "react";
import { SearchIcon } from "../icons/actions";
import { cx } from "../utils/cx";
import styles from "./SearchField.module.css";

export type SearchFieldProps = Omit<ComponentPropsWithRef<"input">, "type" | "size"> & {
  /** Accessible label; visually hidden because the icon and placeholder carry the meaning. */
  label: string;
  /** Applied to the outer field, not the input. */
  className?: string;
};

export function SearchField({ label, className, ...inputProps }: SearchFieldProps) {
  return (
    <label className={cx(styles.field, className)}>
      <SearchIcon className={styles.icon} />
      <span className={styles.visuallyHidden}>{label}</span>
      <input type="search" className={styles.input} {...inputProps} />
    </label>
  );
}
