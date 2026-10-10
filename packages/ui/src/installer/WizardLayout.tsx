import type { ReactNode, Ref } from "react";
import { Brand } from "../components/Brand";
import { cx } from "../utils/cx";
import styles from "./WizardLayout.module.css";

export type WizardLayoutProps = {
  /** Product area shown beside the brand, e.g. "Setup". */
  title: string;
  /** Header status at the end, typically a preview `Badge`. */
  badge?: ReactNode;
  /** Step navigation for the left rail, typically a `Stepper`. */
  steps: ReactNode;
  /** Extra rail content under the steps, such as a notice. */
  aside?: ReactNode;
  /** Pinned bar under the content, typically a `WizardFooter`. */
  footer: ReactNode;
  /** The current step's content. */
  children: ReactNode;
  /** The scrolling content element, so a step change can reset its scroll position. */
  contentRef?: Ref<HTMLDivElement>;
  className?: string;
};

/** Full-window installer frame: brand header, step rail, scrollable step content, and a pinned footer. */
export function WizardLayout({ title, badge, steps, aside, footer, children, contentRef, className }: WizardLayoutProps) {
  return (
    <div className={cx(styles.layout, className)}>
      <header className={styles.header}>
        <div className={styles.headerStart}>
          <Brand size="md" />
          <span className={styles.title}>{title}</span>
        </div>
        {badge ? <div className={styles.headerEnd}>{badge}</div> : null}
      </header>
      <div className={styles.body}>
        <aside className={styles.rail}>
          {steps}
          {aside ? <div className={styles.aside}>{aside}</div> : null}
        </aside>
        <div className={styles.panel}>
          <main ref={contentRef} className={styles.content}>
            <div className={styles.inner}>{children}</div>
          </main>
          <div className={styles.footer}>{footer}</div>
        </div>
      </div>
    </div>
  );
}

export type WizardFooterProps = {
  /** Usually a `Button` that returns to the previous step. */
  back?: ReactNode;
  /** Why the main action is unavailable, or what happens next; announced politely when it changes. */
  status?: ReactNode;
  /** Id for the status text, so a disabled action can reference it with `aria-describedby`. */
  statusId?: string;
  /** Usually the primary `Button` that advances the wizard. */
  next?: ReactNode;
};

/** Wizard footer: back on the start, a status line, and the main action on the end. */
export function WizardFooter({ back, status, statusId, next }: WizardFooterProps) {
  return (
    <div className={styles.footerRow}>
      <div className={styles.footerBack}>{back}</div>
      <p id={statusId} className={styles.footerStatus} role="status" aria-live="polite">
        {status}
      </p>
      <div className={styles.footerNext}>{next}</div>
    </div>
  );
}
