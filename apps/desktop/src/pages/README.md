# Desktop pages

Screens are implemented as feature modules under `../features/` and registered by route in `../app/registry.tsx`; routes, tabs, and screen identity live in `../lib/routes.ts`. This folder keeps the original page-boundary notes for reference.

Screens whose backend exists read live data (see `../features/README.md`); the rest show sample data (`../lib/sample/` and each feature's `sample.ts`) under a "Sample data" badge, and actions that need AiDEX Core show a "Preview" toast instead of claiming an effect. Screens become functional as the corresponding Core APIs land.
