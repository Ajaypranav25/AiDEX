import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import styles from "./CodeBlock.module.css";

export type CodeBlockProps = Omit<ComponentPropsWithRef<"figure">, "children"> & {
  code: string;
  title?: string;
  /** Shown as a label and exposed as `data-language` for syntax highlighters. */
  language?: string;
  /** Header controls, e.g. a copy `IconButton`. */
  actions?: ReactNode;
};

/** Horizontally scrollable monospace code with an optional title, language label, and header actions. */
export function CodeBlock({ code, title, language, actions, className, ...props }: CodeBlockProps) {
  const hasHeader = Boolean(title || language || actions);
  return (
    <figure
      aria-label={title ?? (language ? `${language} code` : "Code")}
      className={cx(styles.block, className)}
      {...props}
    >
      {hasHeader ? (
        <div className={styles.header}>
          {title ? <span className={styles.title}>{title}</span> : null}
          {language ? <span className={styles.language}>{language}</span> : null}
          {actions ? <div className={styles.actions}>{actions}</div> : null}
        </div>
      ) : null}
      {/* Focusable so keyboard users can scroll long lines horizontally. */}
      <pre className={styles.pre} tabIndex={0}>
        <code data-language={language}>{code}</code>
      </pre>
    </figure>
  );
}
