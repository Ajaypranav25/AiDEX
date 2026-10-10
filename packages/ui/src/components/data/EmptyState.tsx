import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import styles from "./EmptyState.module.css";

export type EmptyStateProps = Omit<ComponentPropsWithRef<"div">, "title" | "children"> & {
  /** Decorative icon above the title. */
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  /** Next steps, typically one or two `Button`s. */
  actions?: ReactNode;
  /** Heading level for the title; omit to render a paragraph that leaves the page outline alone. */
  titleLevel?: 2 | 3 | 4;
};

/** Centered placeholder for a page or panel with nothing to show yet. */
export function EmptyState({ icon, title, description, actions, titleLevel, className, ...props }: EmptyStateProps) {
  const Title = titleLevel ? (`h${titleLevel}` as const) : "p";
  return (
    <div className={cx(styles.empty, className)} {...props}>
      {icon ? (
        <span className={styles.icon} aria-hidden="true">
          {icon}
        </span>
      ) : null}
      <Title className={styles.title}>{title}</Title>
      {description ? <p className={styles.description}>{description}</p> : null}
      {actions ? <div className={styles.actions}>{actions}</div> : null}
    </div>
  );
}
