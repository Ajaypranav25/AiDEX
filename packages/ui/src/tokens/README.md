# Design tokens

`tokens.css` defines the shared color, typography, spacing, radius, motion, and chart tokens as `--aidex-*` custom properties. The accent family derives from `--aidex-color-accent` with `color-mix()`, so changing that one token re-themes focus rings, tints, and borders. `../styles.css` imports it alongside the base element styles.
