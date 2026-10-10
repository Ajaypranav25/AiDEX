# Setup steps

Wizard steps for the AiDEX Setup preview, one folder per stage. `App.tsx` owns the wizard state (current step and selections) and `StepContent.tsx` renders the current step. These folders live in `steps/`, not `pages/`, because Next.js treats `src/pages` as Pages Router routes.

Every step is composed only from `@aidex/ui` components (no CSS files, `className`, or `style` here). The flow is a preview: it never scans, formats, encrypts, or writes to a drive, and it must not report a drive as ready. Drives, host details, sizes, and results are sample data from `../lib/setup.ts` and each folder's `sample.ts`.
