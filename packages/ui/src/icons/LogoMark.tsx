import type { IconProps } from "./createIcon";

// Traced from the brand artwork: a 972x1108 frame with 50px strokes, normalized to a 40-unit width.
const LOGO_MARK = {
  viewBox: "0 0 40 45.6",
  strokeWidth: 2.06,
  frame: { x: 1.03, y: 1.03, width: 37.94, height: 43.54, rx: 6.5 },
  eyes: [
    { x: 6.21, y: 20.66, width: 7.86, height: 7.9, rx: 2.47 },
    { x: 25.88, y: 20.66, width: 7.86, height: 7.9, rx: 2.47 },
  ],
  divider:
    "M20 1.03V5.35C20 7.98 23.95 9.3 23.95 11.93C23.95 14.57 20 15.88 20 18.52V28.68C20 31.52 16.34 32.35 16.34 35.19C16.34 38.02 20 38.85 20 41.69V44.57",
} as const;

/** The AiDEX mark: two faces sharing one frame. `size` sets the height; width follows the portrait proportions. */
export function LogoMark({ size = 40, title, ...props }: IconProps) {
  return (
    <svg
      height={size}
      viewBox={LOGO_MARK.viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth={LOGO_MARK.strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      focusable="false"
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      {...props}
    >
      {title ? <title>{title}</title> : null}
      <rect {...LOGO_MARK.frame} />
      {LOGO_MARK.eyes.map((eye) => (
        <rect key={eye.x} {...eye} />
      ))}
      <path d={LOGO_MARK.divider} />
    </svg>
  );
}
