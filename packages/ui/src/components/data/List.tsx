import type { ComponentPropsWithRef, CSSProperties } from "react";
import { cx } from "../../utils/cx";
import styles from "./List.module.css";

export type ListProps = ComponentPropsWithRef<"ul"> & {
  /** Width of each `ListRow` meta column, so columns align across rows (default 120px). */
  metaWidth?: string;
};

/** Ruled list of `ListRow`s for index pages such as history, workspaces, or sessions. */
export function List({ metaWidth, className, style, ...props }: ListProps) {
  const listStyle = metaWidth ? ({ ...style, "--list-meta-width": metaWidth } as CSSProperties) : style;
  // `list-style: none` drops list semantics in Safari; the explicit role restores them.
  return <ul role="list" className={cx(styles.list, className)} style={listStyle} {...props} />;
}
