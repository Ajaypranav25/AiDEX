export const navigationItems = [
  { id: "home", label: "Home", icon: "⌂" },
  { id: "workspace", label: "Workspace", icon: "▧" },
  { id: "sessions", label: "Sessions", icon: "◷" },
  { id: "models", label: "Models", icon: "◈" },
  { id: "agents", label: "Agents", icon: "◇" },
  { id: "skills", label: "Skills", icon: "✳" },
  { id: "mcp", label: "MCP servers", icon: "⌘" },
  { id: "plugins", label: "Plugins", icon: "⊞" },
  { id: "security", label: "Security", icon: "⬡" },
  { id: "remote", label: "Remote", icon: "⌁" },
  { id: "settings", label: "Settings", icon: "⚙" },
] as const;

export type PageId = (typeof navigationItems)[number]["id"];
