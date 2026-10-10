import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import styles from "./Avatar.module.css";

export type AvatarProps = Omit<ComponentPropsWithRef<"span">, "children"> & {
  /** Person or account name; used for the initials and the accessible name. */
  name: string;
  size?: "sm" | "md" | "lg";
};

function initialsOf(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => Array.from(part)[0] ?? "")
    .join("")
    .toUpperCase();
}

export function Avatar({ name, size = "md", className, ...props }: AvatarProps) {
  return (
    <span role="img" aria-label={name} className={cx(styles.avatar, styles[size], className)} {...props}>
      <span aria-hidden="true">{initialsOf(name)}</span>
    </span>
  );
}
