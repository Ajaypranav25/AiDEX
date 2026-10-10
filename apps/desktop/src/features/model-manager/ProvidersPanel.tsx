import { useEffect, useState } from "react";
import { List, ListRow, Notice, SectionHeader } from "@aidex/ui/components";
import { CloudIcon, LockIcon } from "@aidex/ui/icons";
import {
  Button,
  Card,
  IconBadge,
  PasswordField,
  Select,
  Stack,
  Text,
  TextField,
} from "@aidex/ui/primitives";
import { call } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";
import type { ProviderAccount, ProviderPreset } from "@aidex/client/dto/models";
import { useRpc } from "../../hooks/useRpc";
import { useToast } from "../../app/providers/toast";
import { useNavigation } from "../../app/providers/navigation";
import { QueryError } from "../system/QueryState";

export function ProvidersPanel() {
  const { navigate } = useNavigation();
  const accounts = useRpc("model.providers");
  const presets = useRpc("provider.presets");
  const models = useRpc("model.list");
  const notify = useToast();
  const [presetId, setPresetId] = useState("openai");
  const [accountId, setAccountId] = useState("openai");
  const [baseUrl, setBaseUrl] = useState("");
  const [credential, setCredential] = useState("");
  const [modelIds, setModelIds] = useState("");
  const [contextWindow, setContextWindow] = useState("32000");
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [health, setHealth] = useState<Record<string, string>>({});
  const [flowId, setFlowId] = useState<string | null>(null);
  const [authState, setAuthState] = useState<string>();
  useEffect(() => {
    if (!flowId) return;
    let live = true;
    const check = async () => {
      try {
        const status = await call("auth.status", { flow_id: flowId });
        if (!live) return;
        setAuthState(
          status.account_label
            ? `${status.state} · ${status.account_label}`
            : status.state,
        );
        if (status.error) setError(status.error);
        if (status.state !== "pending") {
          setFlowId(null);
          accounts.refetch();
          models.refetch();
        }
      } catch (caught) {
        if (live) setError(toAidexError(caught).message);
      }
    };
    void check();
    const timer = setInterval(() => void check(), 1000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [flowId, accounts.refetch, models.refetch]);
  const available = presets.data?.presets ?? [];
  const selected = available.find((preset) => preset.id === presetId);
  const refresh = () => {
    accounts.refetch();
    models.refetch();
  };
  const changePreset = (id: string) => {
    const preset = available.find((candidate) => candidate.id === id);
    setPresetId(id);
    setAccountId(id);
    setBaseUrl(preset?.base_url ?? "");
    setEditing(null);
    setError(undefined);
  };
  const edit = (account: ProviderAccount) => {
    const descriptor = account.descriptor;
    const preset = available.find(
      (candidate) => candidate.kind === descriptor.kind,
    );
    setPresetId(preset?.id ?? "custom");
    setAccountId(descriptor.id);
    setBaseUrl(descriptor.base_url ?? "");
    const configured =
      models.data?.models.filter(
        (model) =>
          model.model.provider_id === descriptor.id &&
          model.source === "configured",
      ) ?? [];
    setModelIds(configured.map((model) => model.model.model_id).join(", "));
    setContextWindow(String(configured[0]?.context_window ?? 32000));
    setCredential("");
    setEditing(descriptor.id);
    setError(undefined);
  };
  const act = async (job: () => Promise<unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      await job();
      refresh();
    } catch (caught) {
      setError(toAidexError(caught).message);
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    void act(async () => {
      if (!selected) throw new Error("Select a supported provider.");
      const id = accountId.trim();
      if (!/^[a-z0-9_-]{1,64}$/.test(id))
        throw new Error(
          "Use a lowercase account ID with letters, digits, underscores or hyphens.",
        );
      const existing = accounts.data?.providers.find(
        (account) => account.descriptor.id === id,
      );
      const secretName =
        existing?.descriptor.auth.type === "vault_secret"
          ? existing.descriptor.auth.secret_name
          : `provider-${id}`;
      const value = credential;
      setCredential("");
      if (value)
        await call("vault.secret.set", {
          name: secretName,
          value,
          consumers: [`provider:${id}`],
          kind: "api_key",
        });
      else if (!existing)
        throw new Error(
          "Enter the provider credential, or use Connect ChatGPT for OAuth.",
        );
      const ids = modelIds
        .split(/[\n,]/)
        .map((value) => value.trim())
        .filter(Boolean);
      const context = Number(contextWindow);
      if (ids.length && (!Number.isSafeInteger(context) || context < 1))
        throw new Error(
          "Enter a positive context limit for configured models.",
        );
      await call(editing ? "provider.update" : "provider.add", {
        id,
        kind: selected.kind,
        base_url: baseUrl.trim() || selected.base_url || "",
        secret_name: secretName,
        discover_models: selected.discover_models,
        models: ids.map((id) => ({
          id,
          context_window: context,
          tool_calls: selected.tool_calls
            ? ("native" as const)
            : ("none" as const),
        })),
      });
      setEditing(null);
      notify(`Provider account ${id} saved.`);
    });
  return (
    <Stack gap={5}>
      <Card padding="md">
        <Stack gap={3}>
          <SectionHeader title="Connect ChatGPT" />
          <Text>
            Sign in with your ChatGPT account to use an eligible plan in AiDEX.
          </Text>
          {error && (
            <Notice tone="danger" role="alert">
              {error}
            </Notice>
          )}
          <Button
            variant="primary"
            disabled={busy}
            onClick={() =>
              void act(async () => {
                const result = await call("auth.start", {
                  provider_id: "openai",
                  method: "chatgpt",
                });
                setFlowId(result.flow_id);
                setAuthState("pending");
                await call("auth.open", {
                  authorization_url: result.authorization_url,
                });
              })
            }
          >
            Connect ChatGPT
          </Button>
          <Button
            variant="outline"
            disabled={busy}
            onClick={() =>
              void act(async () => {
                if (flowId) await call("auth.cancel", { flow_id: flowId });
                const provider_id = `openai-chatgpt-${crypto.randomUUID().slice(0, 8)}`;
                const result = await call("auth.start", {
                  provider_id,
                  method: "chatgpt",
                });
                setFlowId(result.flow_id);
                setAuthState("pending");
                await call("auth.open", {
                  authorization_url: result.authorization_url,
                });
              })
            }
          >
            Connect another account or workspace
          </Button>
          <Text tone="muted">
            If ChatGPT reports a workspace access error, use a new connection
            and choose a workspace you belong to. Existing connections are
            preserved.
          </Text>
          {authState && <Text>{authState}</Text>}
          {authState?.startsWith("connected") && (
            <Button onClick={() => navigate({ page: "chat" })}>
              Open chat
            </Button>
          )}
          {flowId && (
            <Button
              variant="outline"
              onClick={() =>
                void act(async () => {
                  await call("auth.cancel", { flow_id: flowId });
                  setFlowId(null);
                  setAuthState("cancelled");
                })
              }
            >
              Cancel sign-in
            </Button>
          )}
        </Stack>
      </Card>
      <section aria-labelledby="provider-accounts-heading">
        <SectionHeader
          title={
            <span id="provider-accounts-heading">Configured accounts</span>
          }
          meta={`${accounts.data?.providers.length ?? 0} accounts`}
        />
        <QueryError query={accounts} failure="Cannot list provider accounts" />
        <List>
          {(accounts.data?.providers ?? []).map((account) => (
            <ListRow
              key={account.descriptor.id}
              media={<IconBadge icon={<CloudIcon />} />}
              title={account.descriptor.id}
              subtitle={account.descriptor.base_url ?? account.descriptor.kind}
              meta={[
                health[account.descriptor.id] ??
                  "Configured · live verification pending",
              ]}
              trailing={
                <Stack direction="row" gap={2}>
                  {account.descriptor.auth.type === "vault_secret" &&
                  account.descriptor.auth.secret_name.startsWith("oauth.") ? (
                    <Button
                      variant="ghost"
                      disabled={busy}
                      onClick={() =>
                        void act(async () => {
                          if (flowId)
                            await call("auth.cancel", { flow_id: flowId });
                          const result = await call("auth.start", {
                            provider_id: account.descriptor.id,
                            method: "chatgpt",
                          });
                          setFlowId(result.flow_id);
                          setAuthState("pending");
                          await call("auth.open", {
                            authorization_url: result.authorization_url,
                          });
                        })
                      }
                    >
                      Reconnect
                    </Button>
                  ) : (
                    <Button
                      variant="ghost"
                      disabled={busy}
                      onClick={() => edit(account)}
                    >
                      Edit
                    </Button>
                  )}
                  <Button
                    variant="outline"
                    disabled={busy}
                    onClick={() =>
                      void act(async () => {
                        const result = await call("provider.health", {
                          id: account.descriptor.id,
                        });
                        setHealth((current) => ({
                          ...current,
                          [account.descriptor.id]:
                            result.message ?? result.status,
                        }));
                      })
                    }
                  >
                    Check connection
                  </Button>
                  <Button
                    variant="ghost"
                    disabled={busy}
                    onClick={() =>
                      void act(() =>
                        call("provider.remove", { id: account.descriptor.id }),
                      )
                    }
                  >
                    Remove
                  </Button>
                </Stack>
              }
            />
          ))}
        </List>
        {!accounts.loading && !accounts.data?.providers.length && (
          <Text tone="muted">No accounts connected yet.</Text>
        )}
      </section>
      <Card padding="md">
        <Stack gap={3}>
          <SectionHeader
            title={editing ? `Edit ${editing}` : "Add provider account"}
          />
          {error && (
            <Notice tone="danger" role="alert">
              {error}
            </Notice>
          )}
          <QueryError query={presets} failure="Cannot load provider presets" />
          <Select
            label="Provider"
            value={presetId}
            options={available.map((preset: ProviderPreset) => ({
              value: preset.id,
              label: preset.display_name,
            }))}
            onChange={(event) => changePreset(event.target.value)}
          />
          <TextField
            label="Account ID"
            value={accountId}
            disabled={editing !== null}
            onChange={(event) => setAccountId(event.target.value)}
          />
          <TextField
            label="API endpoint"
            value={baseUrl || selected?.base_url || ""}
            onChange={(event) => setBaseUrl(event.target.value)}
            hint={
              selected?.kind === "azure-openai"
                ? "Include the deployment-specific API path and supported API version."
                : "Custom endpoints must use HTTPS, except local loopback services."
            }
          />
          <PasswordField
            label={
              selected?.kind === "bedrock"
                ? "AWS credentials JSON"
                : editing
                  ? "Replacement credential (leave empty to keep current)"
                  : "API credential"
            }
            value={credential}
            onChange={(event) => setCredential(event.target.value)}
            autoComplete="off"
          />
          {selected?.kind === "bedrock" && (
            <Text tone="muted">
              Use access_key_id, secret_access_key, region, and optional
              session_token fields.
            </Text>
          )}
          <TextField
            label="Configured model IDs"
            value={modelIds}
            onChange={(event) => setModelIds(event.target.value)}
            hint="Comma-separated IDs. Required when the provider does not support model discovery."
          />
          <TextField
            label="Configured model context limit"
            inputMode="numeric"
            value={contextWindow}
            onChange={(event) => setContextWindow(event.target.value)}
            hint="Use the model's documented token limit."
          />
          <Button variant="primary" disabled={busy || !selected} onClick={save}>
            {busy ? "Working…" : "Save account"}
          </Button>
        </Stack>
      </Card>
      <Stack direction="row" gap={2} align="center">
        <LockIcon size={16} />
        <Text tone="muted">
          Credentials are encrypted in the drive vault and excluded from model
          prompts.
        </Text>
      </Stack>
    </Stack>
  );
}
