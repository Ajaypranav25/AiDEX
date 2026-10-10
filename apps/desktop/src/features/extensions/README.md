# Extensions feature

The Plugins screen (route `extensions`) for skills, MCP servers, and plugins, plus its sidebar. Everything is composed from `@aidex/ui`; the data is sample data in `sample.ts` until AiDEX Core exposes installed skills, MCP servers, plugins, and the marketplace.

| File | Shows |
| --- | --- |
| `ExtensionsPage.tsx` | Header with a tab-dependent primary action and underline tabs bound to the route tab (`skills`, `mcp`, `plugins`). Holds enable toggles and the plugin view so they survive tab switches. |
| `SkillsPanel.tsx` | Search and status filter, skills grouped by category with enable switches; "Edit" opens a split list / editor. |
| `SkillEditor.tsx` | Monospace SKILL.md editor with Save (preview toast) and Cancel. |
| `McpPanel.tsx` | Split server list and detail for the selected server. |
| `McpServerDetail.tsx` | Status, transport, command or URL, exposed tools, authentication, recent log, and Start / Restart / Disconnect actions. |
| `PluginsPanel.tsx` | Installed and Marketplace views: plugin cards with permission badges, enable switches, Install, and the sandboxing note. |
| `ExtensionsSidebar.tsx` | Browse (tabs, active by tab) and Enabled shortcuts, which open a tab with an item selected through the route `id`. |
| `useToggles.ts` | In-memory on/off state for skills and plugins. |

Enable switches and selection are local UI state; install, add, save, authenticate, and server actions show a "Preview" toast until Core is connected.
