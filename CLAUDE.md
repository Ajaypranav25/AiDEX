# AiDEX

## Package manager

Bun manages the workspace (`packageManager: bun@1.3.14`). Use `bun install` and `bun run`, never npm, pnpm, or yarn, and commit `bun.lock` together with the `package.json` change that produced it.

## Dev ports

| App | Port | Command |
| --- | --- | --- |
| Desktop (`apps/desktop`) | `127.0.0.1:3311` | `bun run dev:desktop` |
| Installer (`apps/installer`) | `127.0.0.1:3312` | `bun run dev:installer` |

Further local services take ports in sequence from `3313`.

## Desktop UI

Desktop screens must be built from `@aidex/ui` (`packages/ui`) components. Do not add CSS files, class names, or styled markup under `apps/desktop`; screens and layouts only compose `@aidex/ui` primitives, components, desktop shell parts, and icons. When a screen needs a new visual building block, add it to `packages/ui` (styled with the `--aidex-*` tokens) and compose it from there.

Check both after UI changes:

```sh
bun run --cwd packages/ui check
bun run --cwd apps/desktop build
```

## Tests

Unit tests use Vitest and Testing Library and sit next to the code they cover (`*.test.ts`, `*.test.tsx`). Write the failing test first. Desktop tests that need AiDEX Core render inside `aidexWrapper` from `apps/desktop/src/test/aidex.tsx` and mock `@aidex/client/rpc` or `@aidex/client/desktop`.

```sh
bun run test                              # @aidex/client (80% coverage gate), @aidex/ui, apps/desktop
bun run --cwd apps/desktop test:e2e       # Playwright smoke tests in apps/desktop/e2e
```

The smoke tests open the dev server in Chromium outside Tauri, where the app runs on its labelled sample data. Install the browser once with `bunx playwright install chromium` in `apps/desktop`.

## Commits

Subject only, no body: `type(scope): imperative subject`, starting lowercase with no trailing period.

- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`.
- Scope: the app, package, or area changed, e.g. `desktop`, `installer`, `ui`, `cli`, `core`, `protocol`.
- Examples from history: `feat(desktop): add Tauri application shell`, `docs(packages): map shared TypeScript boundaries`.
