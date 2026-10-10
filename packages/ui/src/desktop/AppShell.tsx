import type { ReactNode, Ref } from "react";
import styles from "./AppShell.module.css";

export type AppShellProps = {
  /** Primary icon navigation, usually a `NavRail`. */
  rail: ReactNode;
  /** Optional contextual panel, usually a `Sidebar`. Omit to collapse it. */
  sidebar?: ReactNode;
  /** Top bar of the main region, usually a `TopBar`. */
  header?: ReactNode;
  /** Ref to the scrollable main region, e.g. to reset scroll on navigation. */
  contentRef?: Ref<HTMLElement>;
  children: ReactNode;
};

/** Full-window desktop frame: rail, optional sidebar, and a scrollable main region. */
export function AppShell({ rail, sidebar, header, contentRef, children }: AppShellProps) {
  return (
    <div className={styles.shell}>
      {rail}
      {sidebar}
      <div className={styles.main}>
        {header}
        <main ref={contentRef} className={styles.content}>
          {children}
        </main>
      </div>
    </div>
  );
}
