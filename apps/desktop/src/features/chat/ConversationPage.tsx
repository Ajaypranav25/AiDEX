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
import { useRpc } from "../../hooks/useRpc";
import { useInferenceStatus } from "../../hooks/useInferenceStatus";
import { LocalInferenceNotice } from "../model-manager/LocalInferenceNotice";
import { HistoryMessages, MessageContent } from "../sessions/HistoryMessages";
import styles from "./ConversationPage.module.css";
const permissionOptions = [
  { value: "supervised", label: "Supervised" },
  { value: "auto_accept_edits", label: "Auto-accept edits" },
  { value: "auto", label: "Auto" },
  { value: "full_access", label: "Full access" },
];
export function ConversationPage({
  initialMode,
}: {
  initialMode: ConversationMode;
}) {
  const { route, replace, navigate } = useNavigation();
  const preferences = usePreferences();
  const models = useRpc("model.list");
  const inference = useInferenceStatus(models.refetch);
  const localCatalog = useRpc("inference.catalog", undefined, {
    enabled: inference.data?.state === "stopped",
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
  const [popup, setPopup] = useState<
    "mode" | "model" | "project" | "attach" | null
  >(null);
  const [attachmentPath, setAttachmentPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const createdId = useRef<string | undefined>(undefined);
  const contextTouched = useRef(!!preferences.pendingWorkspaceId);
  const permissionTouched = useRef(false);
  const pendingCommand = useRef<{ content: string; id: string } | null>(null);
  const autoSent = useRef(false);
  const endRef = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [arrivalPrompt] = useState(() => preferences.pendingPrompt);
  const availableModels =
    models.data?.models.filter((m) => m.capabilities.chat) ?? [];
  const currentRef = session.data
    ? `${session.data.provider_id}/${session.data.model_id}`
    : preferences.modelId;
  const selectedModel = availableModels.find(
    (m) => `${m.model.provider_id}/${m.model.model_id}` === currentRef,
  );
  const preferredLocal =
    currentRef?.startsWith("local-llama/") ||
    (!currentRef && models.data?.default_model?.provider_id === "local-llama");
  const installedModel =
    localCatalog.data?.models.find(
      (item) =>
        item.id ===
        (inference.data?.model_id ?? localCatalog.data?.settings?.model_id),
    ) ?? localCatalog.data?.models[0];
  const chosenModel = preferredLocal
    ? inference.data?.state === "ready"
      ? (selectedModel ??
        (!session.data
          ? availableModels.find(
              (item) => item.model.provider_id === "local-llama",
            )
          : undefined))
      : undefined
    : session.data
      ? selectedModel
      : (selectedModel ??
        availableModels.find(
          (m) =>
            m.model.provider_id === models.data?.default_model?.provider_id &&
            m.model.model_id === models.data?.default_model?.model_id,
        ) ??
        availableModels[0]);
  const model =
    chosenModel?.model.provider_id === "local-llama" &&
    inference.data?.state !== "ready"
      ? undefined
      : chosenModel;
  const project = workspaces.data?.workspaces.find((w) => w.id === workspaceId);
  const latestRun = session.data?.runs
    .filter((run) => !run.parent_run_id)
    .at(-1);
  const running =
    !!latestRun &&
    ["pending", "running", "waiting_approval"].includes(latestRun.status);
  const modelName =
    model?.display_name ??
    inference.data?.model_name ??
    installedModel?.name ??
    "Choose a model";
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
  const configuration = (
    nextMode = mode,
    context = workspaceId,
    target = model,
  ): SessionConfig => {
    if (route.id && !session.data)
      throw new Error("The conversation is still loading.");
    if (!target)
      throw new Error("Connect a provider or start a local model first.");
    if (nextMode !== "chat" && !context)
      throw new Error("Choose a project before starting Plan or Agent mode.");
    if (context && !workspaces.data?.workspaces.some((w) => w.id === context))
      throw new Error("Choose a registered project or No project context.");
    return {
      title: session.data?.title ?? draft.trim().slice(0, 72),
      workspace_id: context || null,
      mode: nextMode,
      provider_id: target.model.provider_id,
      model_id: target.model.model_id,
      permission_mode: permission,
      runtime_id: session.data?.runtime_id ?? null,
      instructions:
        session.data?.instructions ?? preferences.settings.instructions,
      fallback_model:
        session.data?.fallback_model ?? preferences.settings.fallback_model,
      max_steps: session.data?.max_steps ?? preferences.settings.max_steps,
      max_tokens: session.data?.max_tokens ?? preferences.settings.max_tokens,
    };
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
      await call(mode === "agent" ? "agent.start" : "message.send", {
        session_id: id,
        content,
        command_id: command.id,
      });
      pendingCommand.current = null;
      setDraft("");
      setAttachmentPath("");
      replace({ page: mode === "agent" ? "agent" : "chat", id });
      session.refetch();
    } catch (caught) {
      setError(toAidexError(caught).message);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    if (
      !arrivalPrompt ||
      autoSent.current ||
      route.id ||
      !model ||
      (workspaceId && !project) ||
      (mode !== "chat" && !workspaceId)
    )
      return;
    autoSent.current = true;
    void submit(arrivalPrompt);
  }, [
    arrivalPrompt,
    route.id,
    model?.model.provider_id,
    model?.model.model_id,
    workspaceId,
    project,
  ]);
  const changeMode = async (value: ConversationMode) => {
    setBusy(true);
    setError(undefined);
    try {
      const config = configuration(value);
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
          configuration: configuration(mode, value),
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
  const dock = (
    <Composer
      size="compact"
      autoFocus
      label="Message"
      value={draft}
      onValueChange={setDraft}
      onSubmit={model ? (text) => void submit(text) : undefined}
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
          <IconButton
            size="sm"
            label={`Model: ${modelName}`}
            icon={<BoxIcon />}
            disabled={busy || running}
            onClick={() => setPopup("model")}
          />
          <IconButton
            size="sm"
            label={project ? `Project: ${project.name}` : "No project context"}
            icon={<FolderIcon />}
            selected={!!project}
            disabled={busy || running}
            onClick={() => setPopup("project")}
          />
          <span className={styles.summary}>
            {mode[0].toUpperCase() + mode.slice(1)} · {modelName}
            {project ? ` · ${project.name}` : ""}
          </span>
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
        {(error ||
          session.error ||
          models.error ||
          inference.error ||
          localCatalog.error) && (
          <Notice tone="danger" role="alert">
            {error ??
              session.error?.message ??
              models.error?.message ??
              inference.error?.message ??
              localCatalog.error?.message}
          </Notice>
        )}
        <LocalInferenceNotice
          status={inference.data}
          onRefresh={inference.refetch}
        />
        {!inference.data && !inference.error && !availableModels.length && (
          <Notice role="status">Checking the local model on this drive…</Notice>
        )}
        {!models.loading &&
          !availableModels.length &&
          !!inference.data &&
          !localCatalog.loading &&
          (inference.data.state !== "stopped" || !!localCatalog.data) &&
          inference.data?.state !== "starting" &&
          inference.data?.state !== "failed" && (
            <Notice>
              <Text>
                {installedModel
                  ? `${installedModel.name} is installed on this drive. Start it in Models and accounts to use local chat.`
                  : "Choose a model to start chatting."}
              </Text>
              <Button
                variant="outline"
                onClick={() => navigate({ page: "models" })}
              >
                Models and accounts
              </Button>
            </Notice>
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
          options={[
            { value: "chat", label: "Chat · no agent tools" },
            { value: "plan", label: "Plan · project context" },
            {
              value: "agent",
              label: "Agent · coding tools",
              disabled: model?.capabilities.tool_calls !== "native",
            },
          ]}
          onChange={(event) =>
            void changeMode(event.target.value as ConversationMode)
          }
        />
        {mode !== "chat" && (
          <Select
            label="Permissions"
            disabled={busy || running}
            value={permission}
            options={permissionOptions}
            onChange={(event) => {
              const value = event.target.value as PermissionMode;
              permissionTouched.current = true;
              if (route.id)
                void act(async () => {
                  await call("agent.configuration.set", {
                    session_id: route.id!,
                    configuration: {
                      ...configuration(),
                      permission_mode: value,
                    },
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
        open={popup === "model"}
        title="Choose a model"
        onClose={() => setPopup(null)}
      >
        {error && <Notice tone="danger">{error}</Notice>}
        <LocalInferenceNotice
          status={inference.data}
          onRefresh={inference.refetch}
        />
        <Select
          label="Model"
          disabled={busy || running}
          value={
            model ? `${model.model.provider_id}/${model.model.model_id}` : ""
          }
          options={availableModels.map((m) => ({
            value: `${m.model.provider_id}/${m.model.model_id}`,
            label: m.display_name,
          }))}
          onChange={(event) => {
            const selected = availableModels.find(
              (m) =>
                `${m.model.provider_id}/${m.model.model_id}` ===
                event.target.value,
            );
            if (!selected) return;
            preferences.setModelId(event.target.value);
            if (route.id)
              void act(() =>
                call("agent.configuration.set", {
                  session_id: route.id!,
                  configuration: configuration(mode, workspaceId, selected),
                }),
              );
            setPopup(null);
          }}
        />
        <Button variant="ghost" onClick={() => navigate({ page: "models" })}>
          Manage models and accounts
        </Button>
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
