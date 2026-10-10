import type { ReactNode } from "react";
import styles from "./Sidebar.module.css";

export type SidebarProps = {
  /** Accessible name for the complementary landmark. */
  label: string;
  /** Fixed content above the scrolling body, such as the brand row and primary action. */
  header?: ReactNode;
  children: ReactNode;
};

/** Contextual panel with a fixed header and a scrolling list of `SidebarSection`s. */
export function Sidebar({ label, header, children }: SidebarProps) {
  return (
    <aside aria-label={label} className={styles.sidebar}>
      {header ? <div className={styles.header}>{header}</div> : null}
      <div className={styles.body}>{children}</div>
    </aside>
  );
}
