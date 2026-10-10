# Desktop pages

`HomePage.tsx` is the implemented shell landing screen. The remaining routes are navigation destinations only; no session, workspace, model, or runtime data is fabricated.

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
