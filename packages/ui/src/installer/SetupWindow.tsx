import type { ReactNode, Ref } from "react";
import { LogoMark } from "../icons/LogoMark";
import styles from "./SetupWindow.module.css";

export type SetupWindowProps = {
  /** Product title in the title bar, e.g. "AiDEX Setup". */
  title: string;
  /** Muted text beside the badge, e.g. "Portable environment installer". */
  subtitle?: ReactNode;
  /** Status badge in the title bar, e.g. a preview `Badge`. */
  badge?: ReactNode;
  /** Progress through the setup, usually a `StepTrack`. */
  steps: ReactNode;
  /** Pinned action bar, usually a `SetupFooter`. */
  footer: ReactNode;
  /** Ref to the scrolling content region, e.g. to reset scroll between steps. */
  contentRef?: Ref<HTMLDivElement>;
  children: ReactNode;
};

/** Centered setup window: title bar, step track, scrolling content column, and pinned footer. */
export function SetupWindow({ title, subtitle, badge, steps, footer, contentRef, children }: SetupWindowProps) {
  return (
    <div className={styles.backdrop}>
      <div className={styles.window}>
        <header className={styles.titleBar}>
          <span className={styles.brand}>
            <LogoMark className={styles.mark} />
            {title}
          </span>
          {subtitle || badge ? (
            <span className={styles.hint}>
              {subtitle}
              {badge}
            </span>
          ) : null}
        </header>
        <div className={styles.steps}>{steps}</div>
        <main ref={contentRef} className={styles.content}>
          <div className={styles.column}>{children}</div>
        </main>
        {footer}
      </div>
    </div>
  );
}

export type SetupFooterProps = {
  /** Progress or status text, announced politely when it changes (e.g. why Continue is unavailable). */
  note: ReactNode;
  noteId?: string;
  /** Action buttons, aligned to the end. */
  children: ReactNode;
};

/** Action bar pinned to the bottom of a `SetupWindow`. */
export function SetupFooter({ note, noteId, children }: SetupFooterProps) {
  return (
    <footer className={styles.footer}>
      <p id={noteId} className={styles.note} aria-live="polite">
        {note}
      </p>
      <div className={styles.actions}>{children}</div>
    </footer>
  );
}
