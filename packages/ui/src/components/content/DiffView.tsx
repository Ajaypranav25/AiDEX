import type { ComponentPropsWithRef, ReactNode } from "react";
import { FileCodeIcon } from "../../icons";
import { cx } from "../../utils/cx";
import a11y from "./a11y.module.css";
import styles from "./DiffView.module.css";

export type DiffLineKind = "context" | "add" | "del";

export type DiffLine = {
  kind: DiffLineKind;
  oldLine?: number;
  newLine?: number;
  text: string;
};

export type DiffViewProps = Omit<ComponentPropsWithRef<"section">, "children"> & {
  fileName: string;
  additions: number;
  deletions: number;
  lines: readonly DiffLine[];
  /** Header controls, e.g. accept, revert, or copy buttons. */
  actions?: ReactNode;
};

const MARKER: Record<DiffLineKind, string> = { context: " ", add: "+", del: "-" };
const SPOKEN_KIND: Partial<Record<DiffLineKind, string>> = { add: "Added: ", del: "Removed: " };

function countLabel(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/** File diff card with +/- markers, unselectable line numbers, and a spoken change summary. */
export function DiffView({ fileName, additions, deletions, lines, actions, className, ...props }: DiffViewProps) {
  // Line-number gutters appear only when the diff supplies those numbers.
  const showOld = lines.some((line) => line.oldLine !== undefined);
  const showNew = lines.some((line) => line.newLine !== undefined);

  return (
    <section aria-label={`Changes to ${fileName}`} className={cx(styles.diff, className)} {...props}>
      <header className={styles.header}>
        <span className={styles.fileIcon} aria-hidden="true">
          <FileCodeIcon />
        </span>
        <span className={styles.fileName} title={fileName}>
          {fileName}
        </span>
        <span className={styles.stats}>
          <span className={styles.added} aria-hidden="true">+{additions}</span>
          <span className={styles.removed} aria-hidden="true">-{deletions}</span>
          <span className={a11y.visuallyHidden}>
            {countLabel(additions, "addition")}, {countLabel(deletions, "deletion")}
          </span>
        </span>
        {actions ? <div className={styles.actions}>{actions}</div> : null}
      </header>
      {/* Focusable so keyboard users can scroll long lines horizontally. */}
      <div className={styles.scroller} tabIndex={0}>
        <div className={styles.lines}>
          {lines.map((line, index) => {
            const spoken = SPOKEN_KIND[line.kind];
            return (
              <div key={index} className={cx(styles.line, styles[line.kind])}>
                {showOld ? <span className={styles.number} aria-hidden="true">{line.oldLine}</span> : null}
                {showNew ? <span className={styles.number} aria-hidden="true">{line.newLine}</span> : null}
                <span className={styles.marker} aria-hidden="true">{MARKER[line.kind]}</span>
                <span className={styles.code}>
                  {spoken ? <span className={a11y.visuallyHidden}>{spoken}</span> : null}
                  {line.text}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
