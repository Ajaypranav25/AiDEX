import { List, ListRow, Notice } from "@aidex/ui/components";
import { Button, Stack, Text } from "@aidex/ui/primitives";
import { call } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";
import { useState } from "react";
import { useRpc } from "../../hooks/useRpc";
import { usePreferences } from "../../app/providers/preferences";
import { useNavigation } from "../../app/providers/navigation";
import { useInferenceStatus } from "../../hooks/useInferenceStatus";

export function ModelInventory({ localOnly = false }: { localOnly?: boolean }) {
  const models = useRpc("model.list");
  const status = useInferenceStatus(models.refetch);
  const { setModelId } = usePreferences();
  const { navigate } = useNavigation();
  const [error, setError] = useState<string>();
  const listed = (models.data?.models ?? []).filter(
    (model) => !localOnly || model.locality === "local",
  );
  return (
    <Stack gap={4}>
      {(error || models.error || status.error) && (
        <Notice tone="danger">
          {error ?? models.error?.message ?? status.error?.message}
        </Notice>
      )}
      {(models.data?.failures ?? []).map((failure) => (
        <Notice key={failure.provider_id} tone="warning">
          {failure.provider_id}: {failure.message}
        </Notice>
      ))}
      <List>
        {listed.map((model) => (
          <ListRow
            key={`${model.model.provider_id}/${model.model.model_id}`}
            title={model.display_name}
            subtitle={`${model.model.provider_id}/${model.model.model_id}`}
            meta={[
              model.locality,
              `${model.context_window || "Unknown"} context tokens`,
              `Tools: ${model.capabilities.tool_calls}`,
            ]}
            onSelect={() =>
              navigate({
                page: "model",
                id: `${model.model.provider_id}/${model.model.model_id}`,
              })
            }
            trailing={
              <Button
                variant="outline"
                onClick={() =>
                  void call("model.use", model.model)
                    .then(() => {
                      setModelId(
                        `${model.model.provider_id}/${model.model.model_id}`,
                      );
                      models.refetch();
                    })
                    .catch((caught) => setError(toAidexError(caught).message))
                }
              >
                Use model
              </Button>
            }
          />
        ))}
      </List>
      {!models.loading && !listed.length && (
        <Text tone="muted">
          {localOnly
            ? status.data?.state === "starting"
              ? `Loading ${status.data.model_name ?? "your local model"}…`
              : status.data?.state === "failed"
                ? "Local chat will be available after the installed model starts."
                : "Start an installed model above to make it available for chat."
            : "No models available. Connect a provider or configure model IDs."}
        </Text>
      )}
    </Stack>
  );
}
