import { useEffect, useRef, useState } from "react";
import { CodeBlock, Composer, Dialog, Notice } from "@aidex/ui/components";
import { DockLayout } from "@aidex/ui/desktop";
import { BoxIcon, ChatIcon, FolderIcon, StopIcon } from "@aidex/ui/icons";
import {
  Button,
  Card,
  IconButton,
  Select,
  Stack,
  Text,
  TextField,
} from "@aidex/ui/primitives";
import { call } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";
import type {
  ConversationMode,
  PermissionMode,
  SessionConfig,
} from "@aidex/client/dto/sessions";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { useRpc } from "../../hooks/useRpc";
import { useProviderCatalog } from "../model-manager/catalog/ProviderCatalog";
import {
  modelTargetOf,
  parseModelKey,
  type ModelTarget,
} from "../model-manager/catalog/modelKeys";
import { LocalInferenceNotice } from "../model-manager/LocalInferenceNotice";
import { HistoryMessages, MessageContent } from "../sessions/HistoryMessages";
import { ConversationModelPicker } from "./ConversationModelPicker";
import { describeCurrentModel } from "./currentModel";
import { ModelStateNotice } from "./ModelStateNotice";
import {
  resolveConversationModel,
  type ResolvedConversationModel,
} from "./resolveConversationModel";
import {
  buildSessionConfig,
  configurationChanged,
  sessionTargetOf,
} from "./sessionConfig";
import styles from "./ConversationPage.module.css";
const permissionOptions = [
  { value: "supervised", label: "Supervised" },
  { value: "auto_accept_edits", label: "Auto-accept edits" },
  { value: "auto", label: "Auto" },
  { value: "full_access", label: "Full access" },
];
const modeLabels: Record<ConversationMode, string> = {
  chat: "Chat",
  plan: "Plan",
  agent: "Agent",
};
const loadingSession: ResolvedConversationModel = {
  key: null,
  state: "none",
  origin: "session",
};
export function ConversationPage({
  initialMode,
}: {
  initialMode: ConversationMode;
}) {
  const { route, replace, navigate } = useNavigation();
  const preferences = usePreferences();
  const catalog = useProviderCatalog();
  const notify = useToast();
  const inference = catalog.inference;
  const localCatalog = useRpc("inference.catalog", undefined, {
    enabled: inference?.state === "stopped",
  });
  const workspaces = useRpc("workspace.list");
  const session = useRpc(
    "conversation.get",
    { id: route.id ?? "" },
    { enabled: !!route.id },
  );
  const [draft, setDraft] = useState(() => preferences.pendingPrompt ?? "");
  const [mode, setMode] = useState(initialMode);
  const [workspaceId, setWorkspaceId] = useState(
    () => preferences.pendingWorkspaceId ?? "",
  );
  const [permission, setPermission] = useState<PermissionMode>(
    preferences.settings.permission_mode ?? "supervised",
  );
  // A new chat's own model choice; existing conversations use their configuration.
  const [draftKey, setDraftKey] = useState<string | null>(null);
  const [popup, setPopup] = useState<"mode" | "project" | "attach" | null>(
    null,
  );
  const [attachmentPath, setAttachmentPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const createdId = useRef<string | undefined>(undefined);
  // What `conversation.create` saved, so a retry after a failed first send can re-apply changes.
  const createdConfig = useRef<SessionConfig | null>(null);
  const contextTouched = useRef(!!preferences.pendingWorkspaceId);
  const permissionTouched = useRef(false);
  const pendingCommand = useRef<{ content: string; id: string } | null>(null);
  const autoSent = useRef(false);
  const endRef = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [arrivalPrompt] = useState(() => preferences.pendingPrompt);

  const resolved =
    route.id && !session.data
      ? loadingSession
      : resolveConversationModel({
          session: session.data,
          draftKey,
          preferenceKey: preferences.modelId,
          registryDefault: catalog.registryDefault,
          models: catalog.chatModels,
          inference,
          runtimes: catalog.runtimes,
        });
  const current = describeCurrentModel(resolved, catalog);
  const modelName = current.name;
  const target = parseModelKey(resolved.key);
  const runtimeDriver =
    resolved.runtime &&
    catalog.runtimeDrivers.find(
      (driver) => driver.driver === resolved.runtime?.driver,
    );
  const installedModel =
    localCatalog.data?.models.find(
      (item) =>
        item.id === (inference?.model_id ?? localCatalog.data?.settings?.model_id),
    ) ?? localCatalog.data?.models[0];
  const project = workspaces.data?.workspaces.find((w) => w.id === workspaceId);
  const latestRun = session.data?.runs
    .filter((run) => !run.parent_run_id)
    .at(-1);
  const running =
    !!latestRun &&
    ["pending", "running", "waiting_approval"].includes(latestRun.status);
  const lastMessage = session.data?.messages.at(-1);
  useEffect(() => {
    const container = endRef.current?.closest("main");
    if (!container) return;
    const track = () => {
      following.current =
        container.scrollHeight - container.scrollTop - container.clientHeight <
        180;
    };
    container.addEventListener("scroll", track, { passive: true });
    return () => container.removeEventListener("scroll", track);
  }, []);
  useEffect(() => {
    const container = endRef.current?.closest("main");
    if (container && following.current)
      container.scrollTop = container.scrollHeight;
  }, [session.data?.messages.length, lastMessage?.content]);
  useEffect(() => {
    preferences.setPendingPrompt(null);
    preferences.setPendingWorkspaceId(null);
  }, []);
  useEffect(() => {
    if (!route.id && preferences.loaded && !permissionTouched.current) {
      setPermission(preferences.settings.permission_mode ?? "supervised");
    }
  }, [route.id, preferences.loaded, preferences.settings.permission_mode]);
  useEffect(() => {
    if (!route.id) return;
    const timer = setInterval(session.refetch, running ? 250 : 1500);
    return () => clearInterval(timer);
  }, [route.id, running, session.refetch]);
  useEffect(() => {
    if (!workspaces.data) return;
    if (session.data) {
      setMode(session.data.mode);
      setPermission(session.data.permission_mode);
      const selected = workspaces.data.workspaces.find(
        (w) =>
          w.id === session.data!.workspace_id ||
          w.name === session.data!.workspace_id,
      );
      setWorkspaceId(selected?.id ?? "");
    } else if (initialMode !== "chat" && !contextTouched.current) {
      const selected = workspaces.data.workspaces.find(
        (w) =>
          w.id === preferences.workspaceId ||
          w.name === preferences.workspaceId,
      );
      setWorkspaceId(selected?.id ?? "");
    }
  }, [
    session.data?.id,
    session.data?.workspace_id,
    session.data?.mode,
    session.data?.permission_mode,
    workspaces.data,
    preferences.workspaceId,
    initialMode,
  ]);
  useEffect(() => {
    if (
      preferences.loaded &&
      workspaces.data &&
      preferences.workspaceId &&
      !workspaces.data.workspaces.some(
        (w) =>
          w.id === preferences.workspaceId ||
          w.name === preferences.workspaceId,
      )
    )
      preferences.setWorkspaceId("");
  }, [preferences.loaded, preferences.workspaceId, workspaces.data]);

  /** The model a configuration write uses: the conversation's own, else the new chat's choice. */
  const currentTarget = (): ModelTarget => {
    if (session.data) return sessionTargetOf(session.data);
    if (resolved.state === "ready" && target) return modelTargetOf(target);
    throw new Error(
      resolved.state === "none"
        ? "Choose a model first."
        : `${modelName} is not available right now. Choose another model.`,
    );
  };
  const configuration = (
    changes: {
      mode?: ConversationMode;
      workspaceId?: string;
      target?: ModelTarget;
      permission?: PermissionMode;
    } = {},
  ): SessionConfig => {
    if (route.id && !session.data)
      throw new Error("The conversation is still loading.");
    return buildSessionConfig({
      session: session.data,
      defaults: preferences.settings,
      title: draft.trim().slice(0, 72),
      mode: changes.mode ?? mode,
      workspaceId: changes.workspaceId ?? workspaceId,
      isRegisteredWorkspace: (id) =>
        !!workspaces.data?.workspaces.some((w) => w.id === id),
      target: changes.target ?? currentTarget(),
      permission: changes.permission ?? permission,
    });
  };
  const act = async (operation: () => Promise<unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      await operation();
      session.refetch();
    } catch (caught) {
      setError(toAidexError(caught).message);
    } finally {
      setBusy(false);
    }
  };
  const submit = async (text: string) => {
    if (busy || running || !text.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      if (resolved.state !== "ready")
        throw new Error(
          resolved.state === "none"
            ? "Choose a model first."
            : resolved.state === "loading-local"
              ? `${modelName} is still loading. Send when it is ready.`
              : `${modelName} is not available right now. Choose another model.`,
        );
      const config = configuration();
      let content = text.trim();
      if (attachmentPath.trim()) {
        if (!workspaceId)
          throw new Error("Choose a project before attaching a project file.");
        content += `\n\nAttached project file ${attachmentPath.trim()}:\n${await readAttachment(workspaceId, attachmentPath.trim())}`;
      }
      let id = route.id ?? createdId.current;
      if (!id) {
        const created = await call("conversation.create", config);
        id = created.id;
        createdId.current = id;
        createdConfig.current = config;
      } else if (!route.id) {
        // Created earlier, but the first send failed: apply any model, mode, or project change since.
        if (
          !createdConfig.current ||
          configurationChanged(createdConfig.current, config)
        ) {
          await call("agent.configuration.set", {
            session_id: id,
            configuration: config,
          });
          createdConfig.current = config;
        }
      } else if (
        session.data &&
        session.data.workspace_id !== config.workspace_id
      ) {
        await call("agent.configuration.set", {
          session_id: id,
          configuration: config,
        });
      }
      const command =
        pendingCommand.current?.content === content
          ? pendingCommand.current
          : { content, id: crypto.randomUUID() };
      pendingCommand.current = command;
      await call(config.mode === "agent" ? "agent.start" : "message.send", {
        session_id: id,
        content,
        command_id: command.id,
      });
      pendingCommand.current = null;
      setDraft("");
      setAttachmentPath("");
      replace({ page: config.mode === "agent" ? "agent" : "chat", id });
      session.refetch();
    } catch (caught) {
      setError(toAidexError(caught).message);
    } finally {
      setBusy(false);
    }
  };
  // A prompt handed over from Home sends itself once an explicit or default model is ready; it never
  // falls through to an arbitrary listed model.
  useEffect(() => {
    if (
      !arrivalPrompt ||
      autoSent.current ||
      route.id ||
      !preferences.loaded ||
      resolved.state !== "ready" ||
      (workspaceId && !project) ||
      (mode !== "chat" && !workspaceId)
    )
      return;
    autoSent.current = true;
    void submit(arrivalPrompt);
  }, [
    arrivalPrompt,
    route.id,
    preferences.loaded,
    resolved.key,
    resolved.state,
    workspaceId,
    project,
  ]);
  const selectModel = async (key: string) => {
    const parsed = parseModelKey(key);
    if (!parsed || key === resolved.key) return;
    const nextTarget = modelTargetOf(parsed);
    const descriptor = catalog.findModel(key);
    const name =
      catalog.models.find((model) => model.key === key)?.name ??
      descriptor?.display_name ??
      parsed.modelId;
    let nextMode = mode;
    let nextPermission = permission;
    let switchedToChat = false;
    if (parsed.kind === "runtime") {
      // External runtimes run as agents with full access inside a project.
      if (!workspaceId) {
        setError("Choose a project before starting Plan or Agent mode.");
        return;
      }
      nextMode = "agent";
      nextPermission = "full_access";
    } else {
      if (target?.kind === "runtime")
        nextPermission = preferences.settings.permission_mode ?? "supervised";
      if (mode === "agent" && descriptor?.capabilities.tool_calls !== "native") {
        nextMode = "chat";
        switchedToChat = true;
      }
    }
    setError(undefined);
    if (route.id) {
      // A per-conversation choice; the default for new chats stays as it is.
      setBusy(true);
      try {
        await call("agent.configuration.set", {
          session_id: route.id,
          configuration: configuration({
            mode: nextMode,
            target: nextTarget,
            permission: nextPermission,
          }),
        });
        setMode(nextMode);
        setPermission(nextPermission);
        session.refetch();
      } catch (caught) {
        setError(toAidexError(caught).message);
        return;
      } finally {
        setBusy(false);
      }
    } else {
      // A new chat keeps the choice as its draft and as the default for the next new chat.
      setDraftKey(key);
      preferences.setModelId(key);
      setMode(nextMode);
      if (nextPermission !== permission) {
        permissionTouched.current = true;
        setPermission(nextPermission);
      }
    }
    if (switchedToChat)
      notify(`Switched to Chat: ${name} cannot use agent tools.`);
  };
  const changeMode = async (value: ConversationMode) => {
    setBusy(true);
    setError(undefined);
    try {
      const config = configuration({ mode: value });
      if (route.id)
        await call("agent.configuration.set", {
          session_id: route.id,
          configuration: config,
        });
      setMode(value);
      setPopup(null);
      session.refetch();
    } catch (caught) {
      setError(toAidexError(caught).message);
    } finally {
      setBusy(false);
    }
  };
  const chooseProject = (value: string) => {
    contextTouched.current = true;
    if (route.id)
      void act(async () => {
        await call("agent.configuration.set", {
          session_id: route.id!,
          configuration: configuration({ workspaceId: value }),
        });
        setWorkspaceId(value);
        setAttachmentPath("");
      });
    else {
      setWorkspaceId(value);
      setAttachmentPath("");
    }
  };
  const pendingApprovals =
    session.data?.approvals.filter((a) => a.status === "pending") ?? [];
  const runtimeModes = runtimeDriver?.modes ?? ["agent"];
  const modeOptions = [
    {
      value: "chat",
      label: "Chat · no agent tools",
      disabled: target?.kind === "runtime" && !runtimeModes.includes("chat"),
    },
    {
      value: "plan",
      label: "Plan · project context",
      disabled: target?.kind === "runtime" && !runtimeModes.includes("plan"),
    },
    {
      value: "agent",
      label: "Agent · coding tools",
      disabled:
        target?.kind === "runtime"
          ? !runtimeModes.includes("agent")
          : resolved.model?.capabilities.tool_calls !== "native",
    },
  ];
  const dock = (
    <Composer
      size="compact"
      autoFocus
      label="Message"
      value={draft}
      onValueChange={setDraft}
      onSubmit={
        resolved.state === "ready" ? (text) => void submit(text) : undefined
      }
      disabled={busy || running}
      onAttach={() => setPopup("attach")}
      placeholder={mode === "chat" ? "Message AiDEX…" : "Describe the task…"}
      attachments={
        attachmentPath ? (
          <span className={styles.attachment}>
            {attachmentPath}
            <IconButton
              size="sm"
              label="Remove attachment"
              icon={<StopIcon />}
              onClick={() => setAttachmentPath("")}
            />
          </span>
        ) : undefined
      }
      toolbarStart={
        <>
          <IconButton
            size="sm"
            label={`Mode: ${mode}`}
            icon={<ChatIcon />}
            disabled={busy || running}
            onClick={() => setPopup("mode")}
          />
          <ConversationModelPicker
            value={resolved.key}
            current={current}
            onSelect={(key) => void selectModel(key)}
            disabled={busy || running || (!!route.id && !session.data)}
          />
          <IconButton
            size="sm"
            label={project ? `Project: ${project.name}` : "No project context"}
            icon={<FolderIcon />}
            selected={!!project}
            disabled={busy || running}
            onClick={() => setPopup("project")}
          />
          <Text as="span" tone="muted">
            {modeLabels[mode]}
            {project ? ` · ${project.name}` : ""}
          </Text>
        </>
      }
      toolbarEnd={
        running ? (
          <IconButton
            size="sm"
            label="Stop generation"
            icon={<StopIcon />}
            onClick={() =>
              void act(() => call("run.cancel", { id: latestRun!.id }))
            }
          />
        ) : undefined
      }
    />
  );
  const localNoticeShown =
    inference?.state === "starting" || inference?.state === "failed";
  return (
    <DockLayout width="prose" dock={dock}>
      <div className={styles.conversation}>
        <header className={styles.header}>
          <h1 tabIndex={-1}>{session.data?.title || "New chat"}</h1>
          {route.id && (
            <IconButton
              size="sm"
              label="Execution details"
              icon={<BoxIcon />}
              onClick={() => navigate({ page: "execution", id: route.id })}
            />
          )}
        </header>
        {(error || session.error || catalog.error || localCatalog.error) && (
          <Notice tone="danger" role="alert">
            {error ??
              session.error?.message ??
              catalog.error ??
              localCatalog.error?.message}
          </Notice>
        )}
        <LocalInferenceNotice
          status={inference}
          onRefresh={catalog.refreshInference}
        />
        {(!route.id || session.data) && (
          <ModelStateNotice
            state={resolved.state}
            name={modelName}
            installedLocalName={installedModel?.name}
            hasModels={catalog.models.length > 0}
            loading={
              catalog.loading ||
              (target?.kind === "runtime" && catalog.runtimesLoading)
            }
            localNoticeShown={localNoticeShown}
          />
        )}
        {!session.data?.messages.length && !route.id && (
          <div className={styles.empty}>
            <ChatIcon size={28} />
            <h2>What would you like to talk about?</h2>
            <Text tone="muted">
              Chat about anything. Add a project only when you need its context.
            </Text>
          </div>
        )}
        <Stack gap={5} role="log" aria-label="Conversation">
          {session.data?.messages
            .filter(
              (m) =>
                m.role !== "system" &&
                (m.role !== "assistant" ||
                  !!m.content ||
                  !!m.tool_calls.length),
            )
            .map((message) => (
              <MessageContent
                key={message.original_index}
                message={message}
                scope={{ session_id: session.data!.id }}
              />
            ))}
        </Stack>
        {session.data?.messages_truncated && (
          <details>
            <summary>Earlier messages</summary>
            <HistoryMessages scope={{ session_id: session.data.id }} />
          </details>
        )}
        {latestRun?.error && (!session.data?.workspace_id || project) && (
          <Notice tone="danger">{latestRun.error}</Notice>
        )}
        {latestRun &&
          ["interrupted", "cancelled"].includes(latestRun.status) && (
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() =>
                void act(() => call("run.resume", { id: latestRun.id }))
              }
            >
              Resume task
            </Button>
          )}
        {pendingApprovals.map((approval) => (
          <Card key={approval.id} padding="md">
            <Stack gap={3}>
              <Text>Approval: {approval.tool_name}</Text>
              <CodeBlock
                language="json"
                code={JSON.stringify(approval.arguments, null, 2)}
              />
              {approval.preview != null && (
                <CodeBlock
                  title="Change preview"
                  language="json"
                  code={JSON.stringify(approval.preview, null, 2)}
                />
              )}
              {(approval.arguments_truncated || approval.preview_truncated) && (
                <Notice tone="warning">
                  Request a smaller action to review it completely.
                </Notice>
              )}
              <Stack direction="row" gap={2}>
                {[true, false].map((approved) => (
                  <Button
                    key={String(approved)}
                    variant={approved ? "primary" : "outline"}
                    disabled={
                      busy ||
                      (approved &&
                        (approval.arguments_truncated ||
                          approval.preview_truncated))
                    }
                    onClick={() =>
                      void act(() =>
                        call("approval.respond", {
                          id: approval.id,
                          fingerprint: approval.fingerprint,
                          approved,
                          command_id: crypto.randomUUID(),
                        }),
                      )
                    }
                  >
                    {approved ? "Approve once" : "Deny"}
                  </Button>
                ))}
              </Stack>
            </Stack>
          </Card>
        ))}
        <div ref={endRef} />
      </div>
      <Dialog
        open={popup === "mode"}
        title="Conversation mode"
        onClose={() => setPopup(null)}
      >
        {error && <Notice tone="danger">{error}</Notice>}
        <Select
          label="Mode"
          disabled={busy || running}
          value={mode}
          options={modeOptions}
          onChange={(event) =>
            void changeMode(event.target.value as ConversationMode)
          }
        />
        {mode !== "chat" && (
          <Select
            label="Permissions"
            disabled={busy || running || target?.kind === "runtime"}
            hint={
              target?.kind === "runtime"
                ? "External runtimes run with full access."
                : undefined
            }
            value={permission}
            options={permissionOptions}
            onChange={(event) => {
              const value = event.target.value as PermissionMode;
              permissionTouched.current = true;
              if (route.id)
                void act(async () => {
                  await call("agent.configuration.set", {
                    session_id: route.id!,
                    configuration: configuration({ permission: value }),
                  });
                  setPermission(value);
                });
              else setPermission(value);
            }}
          />
        )}
        <Text tone="muted">
          Plan and Agent require a project. Chat works independently.
        </Text>
      </Dialog>
      <Dialog
        open={popup === "project" || popup === "attach"}
        title={popup === "attach" ? "Attach project file" : "Project context"}
        onClose={() => setPopup(null)}
      >
        {error && <Notice tone="danger">{error}</Notice>}
        <Select
          label="Project"
          disabled={busy || running}
          value={workspaceId}
          options={[
            { value: "", label: "No project context" },
            ...(workspaces.data?.workspaces ?? []).map((w) => ({
              value: w.id,
              label: w.name,
              disabled: w.availability.state !== "available",
            })),
          ]}
          onChange={(event) => chooseProject(event.target.value)}
        />
        {popup === "attach" ? (
          <>
            <TextField
              label="Relative file path"
              value={attachmentPath}
              onChange={(event) => setAttachmentPath(event.target.value)}
              disabled={!workspaceId}
            />
            <Button
              disabled={!workspaceId || !attachmentPath.trim()}
              onClick={() => setPopup(null)}
            >
              Attach file
            </Button>
          </>
        ) : (
          <Text tone="muted">
            Ordinary chats don't need a project. Choose one to include project
            context.
          </Text>
        )}
        <Button
          variant="ghost"
          onClick={() => navigate({ page: "workspace-create" })}
        >
          Add a workspace
        </Button>
      </Dialog>
    </DockLayout>
  );
}
async function readAttachment(
  workspace_id: string,
  path: string,
): Promise<string> {
  const { callUntyped } = await import("@aidex/client/rpc");
  const file = (await callUntyped("workspace.files.read", {
    workspace_id,
    path,
  })) as {
    content: string | null;
    binary: boolean;
    truncated: boolean;
    lossy_utf8: boolean;
  };
  if (file.binary || file.content === null)
    throw new Error("Choose a text file.");
  if (file.truncated) throw new Error("Choose a smaller file or excerpt.");
  if (file.lossy_utf8)
    throw new Error("Convert the file to UTF-8 before attaching.");
  return file.content;
}
