import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import styles from "./KeyValueList.module.css";

export type KeyValueItem = {
  label: ReactNode;
  value: ReactNode;
};

export type KeyValueListProps = Omit<ComponentPropsWithRef<"dl">, "children"> & {
  items: readonly KeyValueItem[];
};

/** Ruled label/value rows, e.g. detected environment or snapshot details. */
export function KeyValueList({ items, className, ...props }: KeyValueListProps) {
  return (
    <dl className={cx(styles.list, className)} {...props}>
      {items.map((item, index) => (
        // Items are positional and usually static, so the index is a stable key.
        <div key={index} className={styles.row}>
          <dt className={styles.label}>{item.label}</dt>
          <dd className={styles.value}>{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
