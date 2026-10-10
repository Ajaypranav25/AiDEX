import type { ComponentPropsWithRef, ReactNode } from "react";
import { TerminalIcon } from "../../icons";
import { cx } from "../../utils/cx";
import a11y from "./a11y.module.css";
import styles from "./Terminal.module.css";

export type TerminalLineKind = "command" | "output" | "error";

export type TerminalLine = {
  kind: TerminalLineKind;
  text: string;
};

export type TerminalProps = Omit<ComponentPropsWithRef<"figure">, "children"> & {
  lines: readonly TerminalLine[];
  title?: string;
  /** Marker drawn before commands; it is not copied with the text. */
  prompt?: string;
  /** Header controls, e.g. a copy `IconButton`. */
  actions?: ReactNode;
};

const SPOKEN_KIND: Partial<Record<TerminalLineKind, string>> = {
  command: "Command: ",
  error: "Error: ",
};

/** Terminal transcript: commands get an accent prompt marker and errors the danger color plus a spoken prefix. */
export function Terminal({ lines, title, prompt = "$", actions, className, ...props }: TerminalProps) {
  return (
    <figure aria-label={title ?? "Terminal output"} className={cx(styles.terminal, className)} {...props}>
      {title || actions ? (
        <div className={styles.header}>
          <span className={styles.headerIcon} aria-hidden="true">
            <TerminalIcon />
          </span>
          <span className={styles.title}>{title}</span>
          {actions ? <div className={styles.actions}>{actions}</div> : null}
        </div>
      ) : null}
      {/* Focusable so keyboard users can scroll long lines horizontally. */}
      <pre className={styles.body} tabIndex={0}>
        {lines.map((line, index) => {
          const spoken = SPOKEN_KIND[line.kind];
          return (
            <span key={index} className={cx(styles.line, styles[line.kind])}>
              {line.kind === "command" ? (
                <span className={styles.prompt} aria-hidden="true">{`${prompt} `}</span>
              ) : null}
              {spoken ? <span className={a11y.visuallyHidden}>{spoken}</span> : null}
              {line.text}
            </span>
          );
        })}
      </pre>
    </figure>
  );
}
