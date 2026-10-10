import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import styles from "./Kbd.module.css";

export type KbdProps = ComponentPropsWithRef<"kbd">;

export function Kbd({ className, ...props }: KbdProps) {
  return <kbd className={cx(styles.kbd, className)} {...props} />;
}
