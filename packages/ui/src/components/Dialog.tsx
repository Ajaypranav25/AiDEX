"use client";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { XIcon } from "../icons/interface";
import { IconButton } from "../primitives/IconButton";
import { cx } from "../utils/cx";
import styles from "./Dialog.module.css";

export type DialogProps = {
  open: boolean;
  title: string;
  onClose: () => void;
  /** Supporting text under the title, linked as the dialog's description. */
  description?: ReactNode;
  /** Width: `md` 480px (default), `lg` 720px, `xl` 960px; always clamped to the window. */
  size?: "md" | "lg" | "xl";
  children: ReactNode;
};

/** Native modal: traps focus, supports Escape, and restores focus to its trigger. */
export function Dialog({ open, title, onClose, description, size = "md", children }: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      className={cx(styles.dialog, styles[size])}
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className={styles.header}>
        <div className={styles.heading}>
          <h2 id={titleId}>{title}</h2>
          {description ? (
            <div id={descriptionId} className={styles.description}>
              {description}
            </div>
          ) : null}
        </div>
        <IconButton
          label="Close dialog"
          size="sm"
          icon={<XIcon />}
          onClick={onClose}
        />
      </div>
      <div className={styles.body}>{children}</div>
    </dialog>
  );
}
