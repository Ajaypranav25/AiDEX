import type { ReactNode } from "react";
import styles from "./FocusLayout.module.css";

export type FocusLayoutProps = {
  /** Usually a vertically centered `Container`. */
  children: ReactNode;
};

/** Full-window frame for focused flows such as unlock and setup, without the workspace shell. */
export function FocusLayout({ children }: FocusLayoutProps) {
  return <main className={styles.layout}>{children}</main>;
}
