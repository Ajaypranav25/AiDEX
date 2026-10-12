# Shared components

Product-level components shared across desktop, installer, and mobile:

- Root: `Brand`, `Composer`, `Dialog`, `MediaItem`, `InfoCard`, `PageHeader` and `SectionHeader`, `ToastRegion`.
- `ModelPicker/`: the provider-grouped model picker (`ModelPicker`, `ModelPickerTrigger`). Pure search ranking lives in `search.ts` and view derivation in `view.ts`, so both stay testable without a DOM.
- `data/`: lists, key-value rows, setting rows and settings sections, provider rows and tiles, empty states, stat tiles, meters, charts, tables, and entity cards.
- `content/`: plan steps, activity logs, task graphs, diffs, code and terminal blocks, chat messages, file trees, timelines, and the decorative pairing-code pattern.
