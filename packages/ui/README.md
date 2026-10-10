# Shared UI package

`@aidex/ui` holds the design tokens and accessible React components that AiDEX frontends are built from. App screens compose these components; they do not define their own styled markup. Add a component here, rather than in an app, whenever a screen needs a new visual building block.

## Entry points

| Import | Contents |
| --- | --- |
| `@aidex/ui/styles.css` | Design tokens and base element styles. Import once in the app root layout. |
| `@aidex/ui/themes` | `accentPresets`, `DEFAULT_ACCENT`, and `applyAccent(color)` for runtime accent changes. |
| `@aidex/ui/icons` | Project-owned 24×24 stroke icons and the AiDEX `LogoMark`. |
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
| `Switch` | `checked`, `onCheckedChange`, `label` (when no visible `<label htmlFor>`) |
| `SegmentedControl` | `label`, `options`, `value`, `onValueChange`, `size` sm · md (radio-group semantics) |
| `Tabs` + `TabPanel` | `id`, `label`, `items: { value, label, icon?, count? }[]`, `value`, `onValueChange`, `variant` pill · underline; wrap the selected content in `<TabPanel tabsId={id} value={value}>` |
| `ChoiceCardGroup` | `label`, `options: { value, title, description?, icon?, meta?, disabled? }[]`, `value`, `onValueChange`, `columns` 1 · 2 · 3 |
| `SwatchPicker` | `label`, `swatches: { value, label }[]`, `value`, `onValueChange`, `allowCustom` |
| `Badge` | `tone` accent · success · neutral, `icon` |
| `StatusIndicator` | `label`, `tone` success · warning · neutral (dot plus text) |
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
| `MediaItem` | `media`, `title`, `description`, `trailing`, `titleLevel`, `size` md · lg |
| `InfoCard` | `icon`, `status`, `label`, `value`, `detail` |
| `ToastRegion` | `toasts: { id, message, tone? }[]`, `onDismiss` |
| `List` + `ListRow` | `ListRow`: `media`, `title`, `subtitle`, `meta: ReactNode[]`, `trailing`, `selected`, `onSelect` (whole row becomes a button) |
| `KeyValueList` | `items: { label, value }[]` |
| `SettingRow` | `title`, `description`, `control`, `controlId` |
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
| `DiffView` | `fileName`, `additions`, `deletions`, `lines: { kind: context · add · del, oldLine?, newLine?, text }[]`, `actions` |
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
| `WizardLayout` | `title`, `badge`, `steps` (a `Stepper`), `aside`, `footer`, `contentRef`; brand header, step rail, scrolling content, pinned footer |
| `WizardFooter` | `back`, `status` (polite live text, e.g. why Next is disabled), `statusId`, `next` |
| `Stepper` | `label`, `steps: { id, title, description?, status: complete · current · upcoming }[]`, `onSelect` (completed steps only) |

## Checks

```sh
bun run --cwd packages/ui check
```
