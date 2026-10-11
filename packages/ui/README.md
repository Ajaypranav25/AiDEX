# Shared UI package

`@aidex/ui` holds the design tokens and accessible React components that AiDEX frontends are built from. App screens compose these components; they do not define their own styled markup. Add a component here, rather than in an app, whenever a screen needs a new visual building block.

## Entry points

| Import | Contents |
| --- | --- |
| `@aidex/ui/styles.css` | Design tokens and base element styles. Import once in the app root layout. |
| `@aidex/ui/themes` | `accentPresets`, `DEFAULT_ACCENT`, and `applyAccent(color)` for runtime accent changes. |
| `@aidex/ui/icons` | Project-owned 24×24 stroke icons, the AiDEX `LogoMark`, and `BrandIcon` provider logos. |
| `@aidex/ui/primitives` | Controls, form fields, display atoms, and layout. |
| `@aidex/ui/components` | Product components, data display, and content display. |
| `@aidex/ui/desktop` | Desktop frames: shell, navigation, page layouts. |
| `@aidex/ui/installer` | Installer wizard frame. |

The package ships TypeScript source and CSS Modules; consuming Next.js apps list it in `transpilePackages`. Styles reference the `--aidex-*` custom properties in `src/tokens/tokens.css` instead of literal values. The accent family (border, soft, ring, focus) derives from `--aidex-color-accent`, so `applyAccent` re-themes all of it.

## Catalog

Every component accepts its root element's props (including `ref` and `className`) unless noted.

### Primitives

| Component | Key props |
| --- | --- |
| `Button` | `variant` primary · tonal · raised · outline · ghost; `size` md · lg · xl; `shape` rounded · pill; `align` start · center; `icon`, `trailing`, `fullWidth` |
| `IconButton` | `label` (required, also the tooltip), `icon`, `variant` ghost · soft · solid · primary, `size` sm · md · lg · xl, `shape`, `selected` |
| `TextField` | `label`, `labelVisibility` visible · hidden, `hint`, `error`, `leadingIcon`, `trailing`, `shape` rounded · pill, `size` md · lg |
| `SearchField` | `TextField` with a search icon, pill shape, and hidden label |
| `PasswordField` | `TextField` with a reveal toggle (`revealLabel`); spell check and autocorrect are always off |
| `TextArea` | `label`, `labelVisibility`, `hint`, `error`, `monospace` |
| `Select` | `label`, `labelVisibility`, `options: { value, label }[]`, `variant` field · ghost, `hint` |
| `Checkbox` | `label`, `description` |
| `CheckboxGroup` | `label`, `options: { value, title, description?, meta?, disabled? }[]`, `value: string[]`, `onValueChange` (ruled rows) |
| `Switch` | `checked`, `onCheckedChange`, `label` (when no visible `<label htmlFor>`) |
| `SegmentedControl` | `label`, `options`, `value`, `onValueChange`, `size` sm · md (radio-group semantics) |
| `Tabs` + `TabPanel` | `id`, `label`, `items: { value, label, icon?, count? }[]`, `value`, `onValueChange`, `variant` pill · underline; wrap the selected content in `<TabPanel tabsId={id} value={value}>` |
| `ChoiceCardGroup` | `label`, `options: { value, title, description?, icon?, meta?, disabled? }[]`, `value`, `onValueChange`, `columns` 1 · 2 · 3, `variant` cards · rows |
| `SwatchPicker` | `label`, `swatches: { value, label }[]`, `value`, `onValueChange`, `allowCustom` |
| `Badge` | `tone` accent · success · warning · danger · neutral, `size` sm · md, `icon` |
| `StatusIndicator` | `label`, `tone` success · warning · danger · neutral (dot plus text) |
| `StatusDot` | `tone` success · warning · danger · neutral · accent, `size` sm (6px) · md (9px), `label` (announced as an image; without it the dot is decorative and nearby text must name the status) |
| `Popover` | `open`, `onOpenChange`, `anchorRef`, `placement` bottom-start · bottom-end · top-start · top-end (flips to stay on screen), `offset` (6), `label` or `labelledBy`, `role` dialog · menu · listbox, `initialFocusRef`, `container` (portal target; pass the open `<dialog>` inside a modal), `id`. Non-modal: focus moves in on open; Escape closes and returns focus to the anchor; a press outside or focus leaving closes it. Not a root-element passthrough |
| `IconBadge` | `icon`, `tone` neutral · accent, `size` md · lg |
| `ProgressBar` | `label`, `value`, `max`, `showCount` |
| `Divider` | optional `label` (e.g. "or") |
| `Kbd`, `Avatar` | keyboard hint; initials avatar (`name`, `size`) |
| `Card` | `padding` md · lg |
| `Stack` | `direction`, `gap` (0–12, ×4px), `align`, `justify`, `wrap` |
| `Grid` | `columns` 2 · 3 · 4 (collapses by container width), `gap` |
| `Split` | two columns: `emphasis` equal · start · end, `divided`; stacks when narrow |
| `Container` | `size` xs · sm · prose · md · lg, `verticallyCentered` |
| `Heading`, `Text` | `Heading`: `level`, `size` display · title, `align`. `Text`: `as`, `variant` body · lead · label, `tone`, `align`, `measure` |

### Components

| Component | Key props |
| --- | --- |
| `PageHeader` | `title`, `description`, `eyebrow`, `actions`, `meta` (status line), `back: { label, onClick }`, `titleRef` |
| `SectionHeader` | `title`, `level` 2 · 3, `meta`, `actions` |
| `Brand` | `name`, `size` md · lg · xl, `layout` inline · stacked |
| `Composer` | `value`, `onValueChange`, `onSubmit`, `onAttach`, `label`, `placeholder`, `attachments`, `toolbarStart`, `toolbarEnd`, `size` default · compact |
| `Dialog` | `open`, `title`, `onClose`, `description` (linked as the accessible description), `size` md · lg · xl (480 / 720 / 960px, clamped to the window); native modal `<dialog>` |
| `ModelPicker` | `providers`, `models`, `value` (model key), `onValueChange`, `favorites: ReadonlySet<string>`, `onToggleFavorite`, `open`, `onOpenChange`, `anchorRef`, `label`, `onOpenProviderSetup`, `onManageProviders`, `emptyText`, `loading`, `jumpModifierLabel` ("⌘" or "Ctrl"), `placement`, `container`, `id`. See [Model picker](#model-picker) |
| `ModelPickerTrigger` | `provider`, `model`, `placeholder`, `open`, `onToggle`, `size` sm · md, `shortcutLabel` (appended to the tooltip), `disabled`, `ref` (pass it as the picker's `anchorRef`) |
| `Notice` | `tone` info · success · warning · danger, `variant` plain · banner (pass `role="alert"` for errors) |
| `MediaItem` | `media`, `title`, `description`, `trailing`, `titleLevel`, `size` md · lg |
| `InfoCard` | `icon`, `status`, `label`, `value`, `detail` |
| `ToastRegion` | `toasts: { id, message, tone? }[]`, `onDismiss` |
| `List` + `ListRow` | `ListRow`: `media`, `title`, `subtitle`, `meta: ReactNode[]`, `trailing`, `selected`, `onSelect` (whole row becomes a button) |
| `KeyValueList` | `items: { label, value }[]` |
| `SettingRow` | `title`, `description`, `control`, `controlId` |
| `SettingsSection` | `title`, `icon`, `description`, `actions`, `titleLevel` 2 · 3 · 4; bordered, titled group whose children stack with a gap |
| `ProviderRow` | `brand`, `title`, `version`, `headline`, `detail`, `tone` success · warning · danger · neutral (dot only for warning and danger), `selected`, `disabled` (dims, stays selectable), `badges`, `trailing` (outside the select button), `onSelect`. An `<li>` for a `List` |
| `ProviderTile` | `brand`, `name`, `description` (one line), `badges`, `selected` (`aria-pressed`; leave undefined for a plain action), `onSelect`, `disabled`; compact catalog button |
| `EmptyState` | `icon`, `title`, `description`, `actions`, `titleLevel` |
| `StatTile` | `label`, `value`, `hint`, `icon`, `tone` |
| `Meter` | `label`, `value`, `max`, `valueText`, `tone`, `warnAt`, `dangerAt` |
| `SegmentedMeter` | `label`, `segments: { label, value, color? }[]`, `max`, `valueText`, `remainderLabel` |
| `BarChart` | `caption`, `hideCaption`, `data: { label, value, highlight? }[]`, `unit`, `max` |
| `DataTable` | `columns: { key, header, align?, width?, rowHeader?, render? }[]`, `rows`, `rowKey`, `caption`, `hideCaption`, `emptyText` |
| `EntityCard` | `media`, `title`, `subtitle`, `badges`, `description`, `footer`, `selected`, `onSelect` |
| `StepList` | `steps: { title, description?, status: done · active · pending · failed }[]` |
| `ActivityLog` | `entries: { time, text, tone? }[]`, `live` |
| `TaskGraph` | `root`, `nodes: { id, label, icon?, status?, detail? }[]` |
| `CodeDiff` | `fileName`, `language?`, either `oldCode` + `newCode` or a unified `patch`, `layout` unified · split (or `defaultLayout` + `onLayoutChange`), `showLayoutToggle`, `actions`. See [Code diffs](#code-diffs). `getDiffStats(fileName, source)` returns the same `{ additions, deletions }` the header shows |
| `CodeBlock`, `Terminal`, `InlineCode` | `CodeBlock`: `code`, `title`, `language`, `actions`. `Terminal`: `lines: { kind: command · output · error, text }[]`, `title` |
| `ChatMessage` | `role` user · assistant, `meta`, `actions` |
| `FileTree` | `nodes: { id, name, kind: folder · file, children?, meta? }[]`, `selectedId`, `onSelect`, `defaultExpanded` |
| `Timeline` | `items: { id, title, time, description? }[]`, `selectedId`, `onSelect` |
| `QrCodePattern` | `seed`, `size` (decorative stand-in; encodes no data) |

### Desktop

| Component | Key props |
| --- | --- |
| `AppShell` | `rail`, `sidebar`, `header`, `contentRef` |
| `NavRail` | `label`, `items: { id, label, icon }[]`, `activeId`, `onSelect` |
| `Sidebar`, `SidebarSection`, `SidebarItem` | `SidebarItem`: `label`, `icon`, `iconTone`, `active`, `size` md · sm, `count`, `actions`, `onSelect` |
| `TopBar` | `start`, `center`, `end` |
| `PageFrame` | `width` narrow · default · wide; the page body inside `AppShell` |
| `DockLayout` | `dock` (pinned to the bottom of the scroll area), `width` prose · default |
| `FocusLayout` | full-window frame for launch and setup flows |

### Installer

| Component | Key props |
| --- | --- |
| `SetupWindow` | `title`, `subtitle`, `badge`, `steps` (a `StepTrack`), `footer` (a `SetupFooter`), `contentRef`; centered window with title bar, step track, scrolling content column, pinned footer |
| `SetupFooter` | `note` (polite live text), `noteId`, action buttons as children |
| `StepTrack` | `label`, `steps: { id, title, status: complete · current · upcoming, reachable? }[]`, `onSelect` |

### Icons

| Export | Notes |
| --- | --- |
| Stroke icons | `createIcon` 24×24 strokes (`size`, `title`), e.g. `StarIcon` and `StarFilledIcon` for favorites |
| `BrandIcon` | `brand` (glyph key), `size` (20), `tone` brand · mono, `title`; other SVG props pass through. See [Brand icons](#brand-icons) |
| `hasBrandIcon(brand)`, `brandTitle(brand)`, `monogram(label)`, `type BrandId` | glyph lookup, display name, and the fallback initials |

## Brand icons

`BrandIcon` draws provider and agent-runtime logos from `icons/brands/brandGlyphs.ts`: generated, monochrome 24×24 path data (49 brands) from `@lobehub/icons-static-svg`, MIT licensed; see `icons/brands/NOTICE.md`. Do not edit the data by hand. Brand names and logos identify the provider only.

- **Color.** `tone="brand"` (default) colors the glyph with its brand color through the `--aidex-brand-color` custom property; glyphs without a usable color, and `tone="mono"`, inherit `currentColor`. In the light theme the brand color is darkened (`color-mix(in oklab, <brand> 78%, black)`, through the `--aidex-brand-color-strength` token) to keep contrast on light surfaces.
- **Fallback.** A key without a glyph renders a rounded-square monogram of up to two initials from `title`, or from `brandTitle(brand)` when there is no title (`monogram("llama.cpp server")` is "LS"; a single word gives its first two letters).
- **Accessibility.** Without `title` the icon is decorative (`aria-hidden`); with one it is `role="img"` with that name. Next to a visible provider name, leave `title` off.

## Model picker

`ModelPicker` is data only: the app maps its providers and models to `ModelPickerProvider` (`id`, `name`, `brand`, `status` ready · warning · error · disabled · setup, `statusText`, `kind` model · runtime · local, `setupLabel`) and `ModelPickerModel` (`key`, `providerId`, `name`, `shortName`, `description`, `contextWindow`, `capabilities` tools · vision · json · reasoning, `badge` new · default · custom, `unavailable`, `disabledReason`, `legacy`), and persists selection and favorites itself.

- **Layout.** A popover (380px wide, up to 420px tall) with a 44px provider rail (Favorites, then providers; agent runtimes after a divider) and a search field above a listbox. Each rail button shows a status dot; its tooltip and accessible name name any non-ready state ("Groq — Unavailable. 401 Unauthorized").
- **Views.** It opens on Favorites when any listed model is a favorite, otherwise on the selected model's provider (or the first ready one). Favorites are ordered by rail order. A provider view lists favorites, then current models, then a "Legacy models" row that expands the `legacy` ones. Typing searches every provider: tokens are matched against name, short name, provider name, and key (exact, prefix, word boundary, substring, then fuzzy for tokens of three or more characters), favorites are boosted, and ties sort by name. The rail hides while searching.
- **Footer.** A provider that is not ready (or has no models) shows its `statusText` or a fallback with an "Open provider setup" button when `onOpenProviderSetup` is set. `onManageProviders` adds a "Manage providers" bar.
- **Keyboard.** Focus starts in the search field. ArrowUp/ArrowDown move the active option (`aria-activedescendant`), Enter selects it (or toggles the legacy row), Escape closes and returns focus to the trigger. ArrowLeft on an empty query or Shift+Tab moves to the rail, where ArrowUp/ArrowDown/Home/End move, Enter or Space selects, and ArrowRight or typing returns to search. Tab from search reaches the active row's favorite button. While open, Mod+1…Mod+9 choose the first nine enabled rows (hints show when `jumpModifierLabel` is set) and Mod+Shift+ArrowUp/ArrowDown switch provider, skipping disabled ones. Rows with a `disabledReason` are skipped and show the reason as their tooltip.
- **Search helpers.** `rankBySearch(items, query, toSearchable)` and `scoreModelSearch(model, query)` expose the same ranking, for example to filter a model list elsewhere; `formatContextWindow(tokens)` gives "200K" / "1M".

## Code diffs

`CodeDiff` is the one diff component; every diff in an app renders through it. It wraps [`@pierre/diffs`](https://diffs.com) (Shiki-based) and adds an accessible header: file name, `+N −M` counts with a spoken "N additions, M deletions" summary, a unified/split `SegmentedControl`, and an `actions` slot.

- **Client only, lazy.** The header and a placeholder sized to the diff are prerendered (safe under `output: "export"`). After hydration the highlighter chunk (`CodeDiffRenderer`) loads with `React.lazy`; Shiki themes and grammars are further split into on-demand chunks, so a screen without diffs never downloads them.
- **No WebAssembly, no network.** Shiki uses its JavaScript regex engine (`preferredHighlighter: "shiki-js"`), so the Tauri CSP needs no `wasm-unsafe-eval`. Themes and grammars are bundled; nothing is fetched at runtime.
- **Theming.** Syntax colors come from the bundled `pierre-dark` theme. Surface, text, line tints, and fonts map onto `--aidex-*` tokens through the `--aidex-color-diff-*` tokens in `tokens.css`, set as `--diffs-*` custom properties on the library's host element.
- **Accessibility.** Lines carry classic `+`/`-` markers, so changes never rely on color alone. Long lines wrap instead of scrolling, and the library's mouse-only hunk-expand controls are turned off.

## Checks

```sh
bun run --cwd packages/ui check
```
