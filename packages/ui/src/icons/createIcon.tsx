import type { ReactNode, SVGProps } from "react";

export type IconProps = Omit<SVGProps<SVGSVGElement>, "children"> & {
  size?: number | string;
  /** Accessible name. Omit for decorative icons, which are hidden from assistive technology. */
  title?: string;
};

/** Builds a 24x24 stroke icon that inherits `currentColor`. */
export function createIcon(displayName: string, glyph: ReactNode) {
  function Icon({ size = 20, title, strokeWidth = 1.75, ...props }: IconProps) {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
        focusable="false"
        role={title ? "img" : undefined}
        aria-hidden={title ? undefined : true}
        {...props}
      >
        {title ? <title>{title}</title> : null}
        {glyph}
      </svg>
    );
  }
  Icon.displayName = displayName;
  return Icon;
}
