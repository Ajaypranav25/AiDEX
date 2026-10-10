# Diff review feature

Diff and test review (route `review`, `id` = selected file id; defaults to the first file), composed only from `@aidex/ui`.

- `ReviewPage.tsx`: header with Run tests, Reject, and Accept (preview toasts) and a change summary; the selected file in a syntax-highlighted `CodeDiff` (unified or split) with previous/next controls and per-file accept/reject marks held in local state.
- `VerificationSection.tsx`: unit, integration, type check, and lint results, each with a written status.
- `ReviewSidebar.tsx`: changed files (selecting one replaces the route `id`; the count is changed lines) and tools.
- `sample.ts`: before/after contents of each changed file (counts come from `getDiffStats`) and verification results until AiDEX Core exposes workspace diffs and test runs.
