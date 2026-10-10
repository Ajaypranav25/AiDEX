import type { ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./IconBadge.module.css";

export type IconBadgeProps = {
  /** Decorative icon; the surrounding text should carry the meaning. */
  icon: ReactNode;
  tone?: "neutral" | "accent";
  size?: "md" | "lg";
  className?: string;
};

/** Circular tile that frames an icon beside a title. */
export function IconBadge({ icon, tone = "neutral", size = "lg", className }: IconBadgeProps) {
  return <span className={cx(styles.badge, styles[tone], styles[size], className)}>{icon}</span>;
}
