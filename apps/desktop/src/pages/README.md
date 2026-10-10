# Desktop pages

The home dashboard (`../features/home/HomePage.tsx`) is the implemented landing screen. Its sidebar shows static placeholder pinned items, workspaces, and recents from `../lib/placeholders.ts`; nothing is read from or written to a drive, and the composer does not send messages until a Core API exists. The remaining rail destinations are navigation targets only.

Planned page boundaries:
- `workspace`: project selection and workspace overview.
- `sessions`: durable session history and resume controls.
- `models`: configured local and provider model inventory.
- `agents`: agent definitions and orchestration status.
- `skills`: installed skill catalog.
- `mcp`: MCP server configuration and connection status.
- `plugins`: plugin inventory and management.
- `security`: execution policy and approval controls.
- `remote`: paired remote access and live session controls.
- `settings`: desktop and portable-environment preferences.

These become functional when the corresponding Core APIs are available; the shell does not simulate them.
