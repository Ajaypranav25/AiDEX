import { useId, type ReactNode } from "react";
import styles from "./SidebarSection.module.css";

export type SidebarSectionProps = {
  title: string;
  /** `SidebarItem`s. */
  children: ReactNode;
};

/** Titled group of sidebar items; consecutive sections are separated by a divider. */
export function SidebarSection({ title, children }: SidebarSectionProps) {
  const titleId = useId();
  return (
    <section aria-labelledby={titleId} className={styles.section}>
      <h2 id={titleId} className={styles.title}>
        {title}
      </h2>
      <ul className={styles.list}>{children}</ul>
    </section>
  );
}
