import type { ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./MediaItem.module.css";

export type MediaItemProps = {
  /** Leading visual, typically an `IconBadge` or icon. */
  media?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  /** Trailing content such as a `StatusIndicator`. */
  trailing?: ReactNode;
  /** Heading level for the title; omit when the title is not a section heading. */
  titleLevel?: 1 | 2 | 3;
  /** `lg` enlarges the text for standalone summaries. */
  size?: "md" | "lg";
  className?: string;
};

/** Media object: a leading visual beside a title and description, with optional trailing status. */
export function MediaItem({ media, title, description, trailing, titleLevel, size = "md", className }: MediaItemProps) {
  const Title = titleLevel ? (`h${titleLevel}` as const) : "p";
  return (
    <div className={cx(styles.item, styles[size], className)}>
      {media ? <span className={styles.media}>{media}</span> : null}
      <div className={styles.text}>
        <Title className={styles.title}>{title}</Title>
        {description ? <p className={styles.description}>{description}</p> : null}
      </div>
      {trailing ? <span className={styles.trailing}>{trailing}</span> : null}
    </div>
  );
}
