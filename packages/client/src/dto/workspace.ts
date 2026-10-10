// Mirrors `aidex_workspace::Workspace` (crates/workspace/src/model.rs) and the `workspace.*` DTOs in
// crates/aidexd/src/methods/workspace.rs. Hand-written until schema generation exists.

export type WorkspaceKind = { type: "host" } | { type: "drive" } | { type: "ephemeral"; source: string };

export type WorkspaceRoot = { type: "host"; path: string } | { type: "drive"; path: string };

export type WorkspaceAvailability = { state: "available" } | { state: "unavailable"; reason: string };

export type RepositoryInfo = { vcs: "git" | "none"; root: string; is_workspace_root: boolean };

export type Workspace = {
  /** `wsp_<ULID>`. */
  id: string;
  name: string;
  kind: WorkspaceKind;
  root: WorkspaceRoot;
  created_at: string;
  last_opened_at: string | null;
  /** Absolute location on this host (display form). */
  path: string;
  availability: WorkspaceAvailability;
  repository: RepositoryInfo | null;
};

/** Result of `workspace.list`. */
export type WorkspaceList = { workspaces: Workspace[]; selected: string | null };

/** Result of `workspace.selected`. */
export type SelectedWorkspace = { workspace: Workspace | null };
