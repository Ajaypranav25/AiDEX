import type { ComponentPropsWithRef } from "react";
import { cx } from "../utils/cx";
import styles from "./Card.module.css";

export type CardProps = ComponentPropsWithRef<"div"> & {
  padding?: "md" | "lg";
};

/** Bordered surface that groups related content. */
export function Card({ padding = "lg", className, ...props }: CardProps) {
  return <div className={cx(styles.card, styles[padding], className)} {...props} />;
}
