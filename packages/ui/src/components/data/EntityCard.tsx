import type { ComponentPropsWithRef, MouseEvent, ReactNode } from "react";
import { CheckCircleIcon } from "../../icons/actions";
import { Card } from "../../primitives/Card";
import { cx } from "../../utils/cx";
import styles from "./EntityCard.module.css";

export type EntityCardProps = Omit<ComponentPropsWithRef<"div">, "title" | "onSelect" | "children"> & {
  /** Leading visual, typically an `IconBadge` or `Avatar`. */
  media?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  /** Status or category pills, typically `Badge`s. */
  badges?: ReactNode;
  description?: ReactNode;
  /** Actions such as `Button`s; they stay clickable above the card's own target. */
  footer?: ReactNode;
  /** Marks the card as the current choice with the accent border and a check. */
  selected?: boolean;
  /** Makes the whole card a target through its title button. */
  onSelect?: (event: MouseEvent<HTMLButtonElement>) => void;
  titleLevel?: 2 | 3 | 4;
};

/** Catalog card for agents, models, or plugins: media, title, badges, description, and actions. */
export function EntityCard({
  media,
  title,
  subtitle,
  badges,
  description,
  footer,
  selected = false,
  onSelect,
  titleLevel = 3,
  className,
  ...props
}: EntityCardProps) {
  const Title = `h${titleLevel}` as const;
  return (
    <Card
      padding="md"
      aria-current={selected && !onSelect ? "true" : undefined}
      className={cx(styles.card, onSelect && styles.interactive, selected && styles.selected, className)}
      {...props}
    >
      <div className={styles.header}>
        {media ? <span className={styles.media}>{media}</span> : null}
        <div className={styles.heading}>
          <Title className={styles.title}>
            {onSelect ? (
              // Stretched target: the button's ::after covers the card, so the whole card is clickable
              // while the accessible control stays a single, well-named button.
              <button
                type="button"
                aria-current={selected ? "true" : undefined}
                className={styles.titleButton}
                onClick={onSelect}
              >
                {title}
              </button>
            ) : (
              title
            )}
          </Title>
          {subtitle ? <p className={styles.subtitle}>{subtitle}</p> : null}
        </div>
        {selected ? <CheckCircleIcon size={20} className={styles.check} /> : null}
      </div>
      {badges ? <div className={styles.badges}>{badges}</div> : null}
      {description ? <p className={styles.description}>{description}</p> : null}
      {footer ? <div className={styles.footer}>{footer}</div> : null}
    </Card>
  );
}
