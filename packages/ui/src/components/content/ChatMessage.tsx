import type { ComponentPropsWithRef, ReactNode } from "react";
import { cx } from "../../utils/cx";
import styles from "./ChatMessage.module.css";

export type ChatRole = "user" | "assistant";

export type ChatMessageProps = Omit<ComponentPropsWithRef<"article">, "role"> & {
  /** Who sent the message; this is not the ARIA `role` attribute. */
  role: ChatRole;
  /** Supporting details such as the model name or time. */
  meta?: ReactNode;
  /** Message controls such as copy or retry buttons. */
  actions?: ReactNode;
};

const DEFAULT_LABEL: Record<ChatRole, string> = {
  user: "You said",
  assistant: "AiDEX replied",
};

/** One chat turn: user messages are right-aligned bubbles, assistant messages are plain readable prose. */
export function ChatMessage({ role, meta, actions, className, children, ...props }: ChatMessageProps) {
  return (
    <article aria-label={DEFAULT_LABEL[role]} className={cx(styles.message, styles[role], className)} {...props}>
      <div className={role === "user" ? styles.bubble : styles.body}>{children}</div>
      {meta || actions ? (
        <footer className={styles.footer}>
          {meta ? <span className={styles.meta}>{meta}</span> : null}
          {actions ? <div className={styles.actions}>{actions}</div> : null}
        </footer>
      ) : null}
    </article>
  );
}
