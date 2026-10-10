import {
  BoxIcon,
  ChatIcon,
  ClockIcon,
  CloudIcon,
  GridIcon,
  HomeIcon,
  SettingsIcon,
  SmartphoneIcon,
  UsersIcon,
} from "@aidex/ui/icons";

export const navigationItems = [
  { id: "home", label: "Home", Icon: HomeIcon },
  { id: "chat", label: "Chat", Icon: ChatIcon },
  { id: "sessions", label: "Sessions", Icon: ClockIcon },
  { id: "remote", label: "Remote", Icon: SmartphoneIcon },
  { id: "plugins", label: "Plugins", Icon: GridIcon },
  { id: "agents", label: "Agents", Icon: UsersIcon },
  { id: "models", label: "Models", Icon: BoxIcon },
  { id: "mcp", label: "MCP servers", Icon: CloudIcon },
  { id: "settings", label: "Settings", Icon: SettingsIcon },
] as const;

export type PageId = (typeof navigationItems)[number]["id"];
