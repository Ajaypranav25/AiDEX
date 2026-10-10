# Shared packages

`packages/` holds shared code with a concrete cross-application consumer.

- `ui` (`@aidex/ui`): the design tokens and components the desktop and installer screens are built from.
- `client` (`@aidex/client`): typed access to AiDEX Core for both Tauri apps (command wrappers, daemon RPC and subscriptions, errors, DTO mirrors).

The remaining directories are reserved; the protocol schema is maintained in `protocol/`, and application-specific dependencies remain owned by their respective apps.
