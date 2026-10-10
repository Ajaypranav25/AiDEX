# Repository guidance

## Read the local rules first

- Follow the nearest `AGENTS.md` files, `CONTRIBUTING.md`, and the owning package or crate documentation. More specific directory guidance takes precedence over this file.
- Preserve generated framework guidance such as the Next.js agent rules in each app; do not remove generated blocks to silence diffs.

## Keep entry points thin

- Keep `main.rs`, `main.ts`, `index.ts`, and `lib.rs` focused on startup, composition, module declarations, and deliberate public exports. Import or call focused modules instead of putting feature implementations in entry-point files.
- Put behavior in cohesive files and subfolders under `src/`, grouped by feature or responsibility. Split independent behaviors into modules when a file becomes difficult to navigate or review; avoid both monolithic files and one-file-per-trivial-helper fragmentation.
- In Rust crates, keep `main.rs` for executable wiring and `lib.rs` for module declarations and the crate's intentional public API. Put substantial implementation in named modules under `src/`.
- In TypeScript packages, `index.ts` may expose a deliberate public API through re-exports, but implementation belongs in focused source modules. Avoid catch-all barrels, circular imports, and side effects in entry-point files.

## Share UI through `packages/ui`

- Put accessible React components and design tokens shared by multiple frontends in `packages/ui` rather than importing code across app directories or duplicating the same component in each app.
- Place reusable product-level components in `packages/ui/src/components/`; use `primitives/`, `tokens/`, `themes/`, and `icons/` for their corresponding shared responsibilities.
- Keep app-specific screens, routes, and behavior in the owning `apps/<app>/` tree. Keep target-specific UI in `packages/ui/src/desktop/`, `installer/`, or `mobile/` only when that UI is genuinely shared within that target boundary.
- Follow `packages/ui/README.md`: extract a component into the shared package when it has a second consumer. Do not move app-only code into the shared package just to centralize files.
- Compose screens from focused reusable components, use the existing tokens and primitives, and preserve accessible semantics and typed component APIs.

## Maintain clear boundaries

- Organize modules around owned behavior and contracts. Keep package and crate dependencies pointed toward lower-level shared contracts; do not import implementation files from another app or introduce dependency cycles.
- Reuse an existing abstraction when it fits. Add an abstraction only for a real shared responsibility, not speculative reuse.
- Keep changes reviewable, update behavior-focused tests and documentation when behavior changes, and do not commit generated build output, credentials, model files, or user data.
