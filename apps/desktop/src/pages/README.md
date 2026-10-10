# Desktop pages

Screens are implemented as feature modules under `../features/` and registered by route in `../app/registry.tsx`; routes, tabs, and screen identity live in `../lib/routes.ts`. This folder keeps the original page-boundary notes for reference.

Every screen is a UI preview: content comes from sample data (`../lib/sample/` and each feature's `sample.ts`), and actions that need AiDEX Core show a "Preview" toast instead of claiming an effect. Screens become functional as the corresponding Core APIs land.
