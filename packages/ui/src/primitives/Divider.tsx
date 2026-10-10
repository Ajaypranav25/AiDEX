import { cx } from "../utils/cx";
import styles from "./Divider.module.css";

export type DividerProps = {
  /** Optional centered text, e.g. "or" between alternative actions. */
  label?: string;
  className?: string;
};

export function Divider({ label, className }: DividerProps) {
  if (!label) return <hr className={cx(styles.rule, className)} />;
  return (
    <div className={cx(styles.divider, className)}>
      <span className={styles.line} aria-hidden="true" />
      <span className={styles.label}>{label}</span>
      <span className={styles.line} aria-hidden="true" />
    </div>
  );
}
