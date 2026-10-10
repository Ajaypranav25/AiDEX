import type { ReactNode } from "react";
import styles from "./TopBar.module.css";

export type TopBarProps = {
  start?: ReactNode;
  /** Centered slot, typically a `SearchField`; capped to a comfortable width. */
  center?: ReactNode;
  end?: ReactNode;
};

/** Three-slot bar for the top of the main region. */
export function TopBar({ start, center, end }: TopBarProps) {
  return (
    <header className={styles.topBar}>
      <div className={styles.start}>{start}</div>
      <div className={styles.center}>{center}</div>
      <div className={styles.end}>{end}</div>
    </header>
  );
}
