import type { ComponentType } from "react";
import { AgentCenterPage } from "../features/agent-center/AgentCenterPage";
import { AgentCenterSidebar } from "../features/agent-center/AgentCenterSidebar";
import { AgentConfigPage } from "../features/agent-center/AgentConfigPage";
import { AgentPage } from "../features/agent/AgentPage";
import { AgentSidebar } from "../features/agent/AgentSidebar";
import { ExecutionPage } from "../features/agent/ExecutionPage";
import { ExecutionSidebar } from "../features/agent/ExecutionSidebar";
import { ChatPage } from "../features/chat/ChatPage";
import { ChatSidebar } from "../features/chat/ChatSidebar";
import { CheckpointsPage } from "../features/checkpoints/CheckpointsPage";
import { CheckpointsSidebar } from "../features/checkpoints/CheckpointsSidebar";
import { ReviewPage } from "../features/diffs/ReviewPage";
import { ReviewSidebar } from "../features/diffs/ReviewSidebar";
import { ExtensionsPage } from "../features/extensions/ExtensionsPage";
import { ExtensionsSidebar } from "../features/extensions/ExtensionsSidebar";
import { ModelCenterPage } from "../features/model-manager/ModelCenterPage";
import { ModelDetailsPage } from "../features/model-manager/ModelDetailsPage";
import { ModelsSidebar } from "../features/model-manager/ModelsSidebar";
import { RemotePage } from "../features/remote-sessions/RemotePage";
import { RemoteSidebar } from "../features/remote-sessions/RemoteSidebar";
import { SecurityPage } from "../features/security/SecurityPage";
import { SecuritySidebar } from "../features/security/SecuritySidebar";
import { HistoryPage } from "../features/sessions/HistoryPage";
import { HistorySidebar } from "../features/sessions/HistorySidebar";
import { SettingsPage } from "../features/settings/SettingsPage";
import { SettingsSidebar } from "../features/settings/SettingsSidebar";
import { CreateWorkspacePage } from "../features/workspace/CreateWorkspacePage";
import { CreateWorkspaceSidebar } from "../features/workspace/CreateWorkspaceSidebar";
import { WorkspaceOverviewPage } from "../features/workspace/WorkspaceOverviewPage";
import { WorkspaceOverviewSidebar } from "../features/workspace/WorkspaceOverviewSidebar";
import { WorkspacesPage } from "../features/workspace/WorkspacesPage";
import { WorkspacesSidebar } from "../features/workspace/WorkspacesSidebar";
import { HomeSidebar } from "../layouts/HomeSidebar";
import type { PageId } from "../lib/routes";

type Screen = { Page: ComponentType; Sidebar: ComponentType };

/** Page and contextual sidebar for every route except Home, whose page needs session props. */
export const screens: Record<Exclude<PageId, "home">, Screen> = {
  workspaces: { Page: WorkspacesPage, Sidebar: WorkspacesSidebar },
  "workspace-create": { Page: CreateWorkspacePage, Sidebar: CreateWorkspaceSidebar },
  workspace: { Page: WorkspaceOverviewPage, Sidebar: WorkspaceOverviewSidebar },
  chat: { Page: ChatPage, Sidebar: ChatSidebar },
  agent: { Page: AgentPage, Sidebar: AgentSidebar },
  execution: { Page: ExecutionPage, Sidebar: ExecutionSidebar },
  review: { Page: ReviewPage, Sidebar: ReviewSidebar },
  history: { Page: HistoryPage, Sidebar: HistorySidebar },
  checkpoints: { Page: CheckpointsPage, Sidebar: CheckpointsSidebar },
  agents: { Page: AgentCenterPage, Sidebar: AgentCenterSidebar },
  "agent-config": { Page: AgentConfigPage, Sidebar: AgentCenterSidebar },
  models: { Page: ModelCenterPage, Sidebar: ModelsSidebar },
  model: { Page: ModelDetailsPage, Sidebar: ModelsSidebar },
  extensions: { Page: ExtensionsPage, Sidebar: ExtensionsSidebar },
  security: { Page: SecurityPage, Sidebar: SecuritySidebar },
  remote: { Page: RemotePage, Sidebar: RemoteSidebar },
  settings: { Page: SettingsPage, Sidebar: SettingsSidebar },
};

export const homeSidebar = HomeSidebar;
