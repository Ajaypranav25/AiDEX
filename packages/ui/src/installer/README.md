# Installer UI

Installer-specific UI components belong here. Keep cross-device components in `../components/`.

| Component | Key props |
| --- | --- |
| `SetupWindow` | `title`, `subtitle`, `badge`, `steps` (a `StepTrack`), `footer` (a `SetupFooter`), `contentRef`; a centered window with title bar, step track, scrolling content column, and pinned footer |
| `SetupFooter` | `note` (polite live text, e.g. step progress or why Continue is unavailable), `noteId`, action buttons as children |
| `StepTrack` | `label`, `steps: { id, title, status: complete · current · upcoming, reachable? }[]`, `onSelect` (reachable, non-current steps only) |
