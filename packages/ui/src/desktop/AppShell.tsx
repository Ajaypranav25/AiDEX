import type { ReactNode } from "react";
import styles from "./AppShell.module.css";

export type AppShellProps = {
  /** Primary icon navigation, usually a `NavRail`. */
  rail: ReactNode;
  /** Optional contextual panel, usually a `Sidebar`. Omit to collapse it. */
  sidebar?: ReactNode;
  /** Top bar of the main region, usually a `TopBar`. */
  header?: ReactNode;
  children: ReactNode;
};

/** Full-window desktop frame: rail, optional sidebar, and a scrollable main region. */
export function AppShell({ rail, sidebar, header, children }: AppShellProps) {
  return (
    <div className={styles.shell}>
      {rail}
      {sidebar}
      <div className={styles.main}>
        {header}
        <main className={styles.content}>{children}</main>
      </div>
    </div>
  );
}
