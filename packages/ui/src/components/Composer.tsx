"use client";

import type { FormEvent, KeyboardEvent, MouseEvent, ReactNode, Ref, SyntheticEvent } from "react";
import { ArrowUpIcon, PaperclipIcon } from "../icons/actions";
import { IconButton } from "../primitives/IconButton";
import { cx } from "../utils/cx";
import { commandMenuFieldProps, type CommandMenuFieldState } from "./CommandMenu/model";
import styles from "./Composer.module.css";

export type ComposerProps = {
  value: string;
  onValueChange: (value: string) => void;
  /** Send is enabled only when a handler is provided and the draft is not blank. */
  onSubmit?: (value: string) => void;
  /** Attach is enabled only when a handler is provided. */
  onAttach?: () => void;
  /** Accessible name for the message field. */
  label: string;
  placeholder?: string;
  attachLabel?: string;
  submitLabel?: string;
  disabled?: boolean;
  autoFocus?: boolean;
  textareaRef?: Ref<HTMLTextAreaElement>;
  /** The composer surface; pass it as a `CommandMenu` `anchorRef` so the menu sits on it and spans its width. */
  ref?: Ref<HTMLFormElement>;
  /**
   * Runs before the composer's own key handling; call `event.preventDefault()` to stop Enter from
   * sending. An open `CommandMenu` handles its keys before this runs.
   */
  onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  /** Caret position (selection end) after typing, clicking, or moving it, e.g. to detect a `$` or `/` trigger. */
  onSelectionChange?: (caret: number) => void;
  /** Ties the field to a `CommandMenu` (`aria-controls`, `aria-activedescendant`) while focus stays in it. */
  suggestions?: CommandMenuFieldState;
  /** A popup for the field, typically a `CommandMenu`; it portals, so placing it here only groups the code. */
  menu?: ReactNode;
  /** Content above the text, e.g. attached-file chips. */
  attachments?: ReactNode;
  /** Controls after the attach button, e.g. a Chat/Agent switch. */
  toolbarStart?: ReactNode;
  /** Controls before the send button, e.g. a model picker. */
  toolbarEnd?: ReactNode;
  /** `compact` is a single-line composer docked under a page. */
  size?: "default" | "compact";
  className?: string;
};

/** Multi-line prompt input with attach and send actions. Enter sends; Shift+Enter adds a line. */
export function Composer({
  value,
  onValueChange,
  onSubmit,
  onAttach,
  label,
  placeholder,
  attachLabel = "Attach files",
  submitLabel = "Send",
  disabled = false,
  autoFocus = false,
  textareaRef,
  ref,
  onKeyDown,
  onSelectionChange,
  suggestions,
  menu,
  attachments,
  toolbarStart,
  toolbarEnd,
  size = "default",
  className,
}: ComposerProps) {
  const canSubmit = onSubmit !== undefined && !disabled && value.trim().length > 0;

  const submit = () => {
    if (canSubmit) onSubmit(value);
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    submit();
  };

  // Enter keeps its newline behavior unless this composer can actually send.
  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || !onSubmit || disabled) return;
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
    event.preventDefault();
    submit();
  };

  const reportCaret = (event: SyntheticEvent<HTMLTextAreaElement>) => {
    onSelectionChange?.(event.currentTarget.selectionEnd);
  };

  // Clicks on the padded surface around the field should still place the caret.
  const focusField = (event: MouseEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    event.preventDefault();
    event.currentTarget.closest("form")?.querySelector("textarea")?.focus();
  };

  return (
    <form
      ref={ref}
      className={cx(styles.composer, size === "compact" && styles.compact, disabled && styles.disabled, className)}
      onSubmit={handleSubmit}
      onMouseDown={focusField}
    >
      {attachments ? <div className={styles.attachments}>{attachments}</div> : null}
      <textarea
        ref={textareaRef}
        className={styles.field}
        value={value}
        placeholder={placeholder}
        aria-label={label}
        rows={size === "compact" ? 1 : 2}
        disabled={disabled}
        autoFocus={autoFocus}
        onChange={(event) => {
          onValueChange(event.target.value);
          reportCaret(event);
        }}
        onSelect={reportCaret}
        onKeyDown={handleKeyDown}
        {...(suggestions ? commandMenuFieldProps(suggestions) : null)}
      />
      <div className={styles.toolbar} onMouseDown={focusField}>
        <div className={styles.group}>
          <IconButton
            label={attachLabel}
            icon={<PaperclipIcon />}
            variant="soft"
            shape="circle"
            size={size === "compact" ? "md" : "lg"}
            disabled={disabled || !onAttach}
            onClick={onAttach}
          />
          {toolbarStart}
        </div>
        <div className={styles.group}>
          {toolbarEnd}
          <IconButton
            type="submit"
            label={submitLabel}
            icon={<ArrowUpIcon />}
            variant={canSubmit ? "primary" : "solid"}
            shape="circle"
            size={size === "compact" ? "md" : "lg"}
            disabled={!canSubmit}
          />
        </div>
      </div>
      {menu}
    </form>
  );
}
