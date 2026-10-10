import type { ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./DockLayout.module.css";

export type DockLayoutProps = {
  /** Scrolling content, e.g. a conversation or a page. */
  children: ReactNode;
  /** Content pinned to the bottom of the scroll area, usually a `Composer`. */
  dock: ReactNode;
  /** Width of the docked content: `prose` for conversations, `default` to match a `PageFrame`. */
  width?: "prose" | "default";
};

/** Page body with a bottom-docked control that stays visible while the content scrolls. */
export function DockLayout({ children, dock, width = "prose" }: DockLayoutProps) {
  return (
    <div className={styles.layout}>
      <div className={styles.body}>{children}</div>
      <div className={styles.dock}>
        <div className={cx(styles.dockInner, styles[width])}>{dock}</div>
      </div>
    </div>
  );
}
