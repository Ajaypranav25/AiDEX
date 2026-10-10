import type { ComponentPropsWithRef } from "react";
import { LogoMark } from "../icons/LogoMark";
import { cx } from "../utils/cx";
import styles from "./Brand.module.css";

export type BrandProps = Omit<ComponentPropsWithRef<"span">, "children"> & {
  name?: string;
  size?: "md" | "lg";
};

/** Logo mark with the product wordmark. */
export function Brand({ name = "AiDEX", size = "lg", className, ...props }: BrandProps) {
  return (
    <span className={cx(styles.brand, styles[size], className)} {...props}>
      <LogoMark className={styles.mark} />
      <span className={styles.wordmark}>{name}</span>
    </span>
  );
}
