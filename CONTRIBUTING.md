# Contributing

## Before changing code

- Keep desktop and installer applications independently installable; shared code belongs in a deliberately versioned or workspace-managed package, not an application-relative import.
- Treat protocol schemas as cross-language contracts. Update the schema and corresponding typed declarations together when the wire format changes.
- Do not commit credentials, user data, model weights, generated binaries, or build output. Use documented examples with synthetic values when examples are needed.
- Do not describe planned integrations or packaging capabilities as implemented.

## Changes and review

Keep changes focused, explain user-visible or protocol-impacting behavior in the pull request, and identify any required platform/toolchain assumptions. Add or update focused tests when a runnable test harness exists. Run the checks documented by the owning app or crate before requesting review; do not assume a repository-wide command is available.

For security-sensitive changes, report the affected component and impact privately to the maintainers rather than publishing exploit details in an issue.
