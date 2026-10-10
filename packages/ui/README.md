# Shared UI package

`@aidex/ui` holds the design tokens and accessible React components that AiDEX frontends are built from. App screens compose these components; they do not define their own styled markup. Add a component here, rather than in an app, whenever a screen needs a new visual building block.

## Entry points

| Import | Contents |
| --- | --- |
| `@aidex/ui/styles.css` | Design tokens and base element styles. Import once in the app root layout. |
| `@aidex/ui/icons` | Project-owned SVG icons and the AiDEX logo mark. |
| `@aidex/ui/primitives` | Controls and layout: `Button`, `IconButton`, `Kbd`, `Avatar`, `SearchField`, `SegmentedControl`, `Stack`, `Container`, `Heading`, `Text`. |
| `@aidex/ui/components` | Cross-device product components: `Brand`, `Composer`. |
| `@aidex/ui/desktop` | Desktop shell: `AppShell`, `NavRail`, `Sidebar`, `SidebarSection`, `SidebarItem`, `TopBar`. |

The package ships TypeScript source and CSS Modules; consuming Next.js apps list it in `transpilePackages`. Styles reference the `--aidex-*` custom properties in `src/tokens/tokens.css` instead of literal values.

## Checks

```sh
bun run --cwd packages/ui check
```
