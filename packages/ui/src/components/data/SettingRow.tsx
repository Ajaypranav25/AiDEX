import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import styles from "./SettingRow.module.css";

export type SettingRowProps = Omit<ComponentPropsWithRef<"div">, "title" | "children"> & {
  title: ReactNode;
  description?: ReactNode;
  /** The setting's control, e.g. a `Switch`, `Select`, or `Button`. */
  control: ReactNode;
  /**
   * Id of the control. When set, the title becomes its `<label>` and the description gets the id
   * `${controlId}-description` for the control's `aria-describedby`. Without it, label the control yourself.
   */
  controlId?: string;
};

/** Settings line: title and description on the left, the control on the right. */
export function SettingRow({ title, description, control, controlId, className, ...props }: SettingRowProps) {
  return (
    <div className={cx(styles.row, className)} {...props}>
      <div className={styles.text}>
        {controlId ? (
          <label className={styles.title} htmlFor={controlId}>
            {title}
          </label>
        ) : (
          <span className={styles.title}>{title}</span>
        )}
        {description ? (
          <p id={controlId ? `${controlId}-description` : undefined} className={styles.description}>
            {description}
          </p>
        ) : null}
      </div>
      <div className={styles.control}>{control}</div>
    </div>
  );
}
