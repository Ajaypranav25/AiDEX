"use client";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { XIcon } from "../icons/interface";
import { IconButton } from "../primitives/IconButton";
import styles from "./Dialog.module.css";

/** Native modal: traps focus, supports Escape, and restores focus to its trigger. */
export function Dialog({
  open,
  title,
  onClose,
  children,
}: {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      className={styles.dialog}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className={styles.header}>
        <h2 id={titleId}>{title}</h2>
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
