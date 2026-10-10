import type { ComponentPropsWithRef } from "react";
import { cx } from "../../utils/cx";
import styles from "./InlineCode.module.css";

export type InlineCodeProps = ComponentPropsWithRef<"code">;

/** Short code fragment set inline with running text. */
export function InlineCode({ className, ...props }: InlineCodeProps) {
  return <code className={cx(styles.code, className)} {...props} />;
}
