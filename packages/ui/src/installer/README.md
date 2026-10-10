# Installer UI

Installer-specific UI components belong here. Keep cross-device components in `../components/`.

| Component | Key props |
| --- | --- |
| `WizardLayout` | `title`, `badge`, `steps` (rail, typically a `Stepper`), `aside` (under the steps), `footer`, `contentRef`; full-window frame with a brand header, step rail, scrolling content, and pinned footer |
| `WizardFooter` | `back`, `status` (polite live text, e.g. why Next is disabled), `statusId`, `next` |
| `Stepper` | `label`, `steps: { id, title, description?, status: complete · current · upcoming }[]`, `onSelect` (completed steps only) |
