import type { CSSProperties, SVGProps } from "react";
import { cx } from "../../utils/cx";
import { brandGlyphs, type BrandGlyph, type BrandId } from "./brandGlyphs";
import styles from "./BrandIcon.module.css";

export type { BrandId };

export type BrandIconProps = Omit<SVGProps<SVGSVGElement>, "children"> & {
  /** Brand glyph key such as `openai` or `claude`; unknown keys render a monogram. */
  brand: string;
  size?: number | string;
  /** `brand` (default) uses the brand color when the glyph has one; `mono` inherits `currentColor`. */
  tone?: "brand" | "mono";
  /** Accessible name. Omit for decorative icons next to a visible name. Also feeds the monogram. */
  title?: string;
};

function glyphFor(brand: string): BrandGlyph | undefined {
  return Object.prototype.hasOwnProperty.call(brandGlyphs, brand)
    ? (brandGlyphs as Record<string, BrandGlyph>)[brand]
    : undefined;
}

/** True when `brand` has a bundled glyph; other keys render a monogram. */
export function hasBrandIcon(brand: string): brand is BrandId {
  return glyphFor(brand) !== undefined;
}

/** Display name of a brand glyph, or a title-cased form of the key (`my-provider` → "My Provider"). */
export function brandTitle(brand: string): string {
  const glyph = glyphFor(brand);
  if (glyph) return glyph.title;
  return brand
    .split(/[\s_-]+/u)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** Up to two initials: the first two characters of a single word, else the first character of the first two words. */
export function monogram(label: string): string {
  const words = label.trim().split(/[\s_-]+/u).filter(Boolean);
  if (words.length === 0) return "?";
  const initials =
    words.length === 1 ? Array.from(words[0]).slice(0, 2).join("") : words.slice(0, 2).map((word) => Array.from(word)[0]).join("");
  return initials.toUpperCase();
}

/** Provider or runtime logo from the bundled brand glyphs, with a monogram fallback. */
export function BrandIcon({ brand, size = 20, tone = "brand", title, className, style, ...props }: BrandIconProps) {
  const glyph = glyphFor(brand);
  const a11y = title
    ? ({ role: "img", "aria-label": title } as const)
    : ({ "aria-hidden": true } as const);

  if (!glyph) {
    const letters = monogram(title ?? brandTitle(brand));
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        focusable="false"
        className={cx(styles.icon, styles.monogram, className)}
        style={style}
        {...a11y}
        {...props}
      >
        {title ? <title>{title}</title> : null}
        <rect className={styles.monogramTile} x="1" y="1" width="22" height="22" rx="6" />
        <text
          className={styles.monogramText}
          x="12"
          y="12.5"
          textAnchor="middle"
          dominantBaseline="central"
          fontSize={letters.length > 1 ? 10 : 12}
        >
          {letters}
        </text>
      </svg>
    );
  }

  const colored = tone === "brand" && glyph.color !== null;
  const iconStyle = colored ? ({ ...style, "--aidex-brand-color": glyph.color } as CSSProperties) : style;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      focusable="false"
      className={cx(styles.icon, colored && styles.brand, className)}
      style={iconStyle}
      {...a11y}
      {...props}
    >
      {title ? <title>{title}</title> : null}
      {glyph.paths.map((path, index) => (
        <path
          // Paths are fixed generated data; their order is their identity.
          key={index}
          d={path.d}
          fillRule={path.evenOdd ? "evenodd" : undefined}
          clipRule={path.evenOdd ? "evenodd" : undefined}
          fillOpacity={path.opacity}
        />
      ))}
    </svg>
  );
}
