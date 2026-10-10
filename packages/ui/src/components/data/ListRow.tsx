import type { ComponentPropsWithRef, MouseEvent, ReactNode } from "react";
import { ChevronRightIcon } from "../../icons/interface";
import { cx } from "../../utils/cx";
import styles from "./ListRow.module.css";

export type ListRowProps = Omit<ComponentPropsWithRef<"li">, "title" | "onSelect" | "children"> & {
  /** Leading visual, typically an icon or `IconBadge`. */
  media?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  /** Secondary facts shown as aligned muted columns; they wrap under the title in narrow lists. */
  meta?: readonly ReactNode[];
  /** Independent controls such as a `Button`; they stay outside the row's own click target. */
  trailing?: ReactNode;
  /** Marks the row as the current item. */
  selected?: boolean;
  /** Turns the row's main area into a button that fills the row. */
  onSelect?: (event: MouseEvent<HTMLButtonElement>) => void;
};

/** One row of a `List`: media, title and subtitle, meta columns, and trailing controls. */
export function ListRow({
  media,
  title,
  subtitle,
  meta,
  trailing,
  selected = false,
  onSelect,
  className,
  ...props
}: ListRowProps) {
  const content = (
    <>
      {media ? <span className={styles.media}>{media}</span> : null}
      <span className={styles.body}>
        <span className={styles.title}>{title}</span>
        {subtitle ? <span className={styles.subtitle}>{subtitle}</span> : null}
        {meta?.length ? (
          <span className={styles.meta}>
            {meta.map((item, index) => (
              // Meta entries are positional columns, so the index is their identity.
              <span key={index} className={styles.metaCell}>
                {item}
              </span>
            ))}
          </span>
        ) : null}
      </span>
    </>
  );

  return (
    <li
      aria-current={selected && !onSelect ? "true" : undefined}
      className={cx(styles.row, onSelect && styles.interactive, selected && styles.selected, className)}
      {...props}
    >
      {onSelect ? (
        <button
          type="button"
          aria-current={selected ? "true" : undefined}
          className={cx(styles.main, styles.action)}
          onClick={onSelect}
        >
          {content}
          <ChevronRightIcon size={16} className={styles.chevron} />
        </button>
      ) : (
        <div className={styles.main}>{content}</div>
      )}
      {trailing ? <span className={styles.trailing}>{trailing}</span> : null}
    </li>
  );
}
