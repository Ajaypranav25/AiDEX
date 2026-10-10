import type { ComponentPropsWithoutRef, ComponentPropsWithRef, ElementType } from "react";
import { cx } from "../utils/cx";
import styles from "./Typography.module.css";

type TextAlign = "start" | "center";

export type HeadingProps = ComponentPropsWithRef<"h1"> & {
  level?: 1 | 2 | 3 | 4;
  size?: "display" | "title";
  align?: TextAlign;
};

/** Semantic heading whose visual size is independent of its level. */
export function Heading({ level = 2, size = "title", align = "start", className, ...props }: HeadingProps) {
  const Tag: ElementType = `h${level}`;
  return <Tag className={cx(styles.heading, styles[size], styles[align], className)} {...props} />;
}

// No ref: its element type would depend on `as`.
export type TextProps = ComponentPropsWithoutRef<"p"> & {
  as?: "p" | "span" | "div";
  variant?: "body" | "lead" | "label";
  /** Line-length cap for `lead` text: `narrow` keeps intros to a short measure. */
  measure?: "narrow" | "wide";
  tone?: "default" | "secondary" | "muted" | "accent";
  align?: TextAlign;
};

export function Text({
  as = "p",
  variant = "body",
  tone = "default",
  align = "start",
  measure = "narrow",
  className,
  ...props
}: TextProps) {
  const Tag: ElementType = as;
  return <Tag className={cx(styles.text, styles[variant], styles[tone], styles[align], variant === "lead" && styles[measure], className)} {...props} />;
}
