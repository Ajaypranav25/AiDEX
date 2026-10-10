import { Notice, PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { Button, Stack, Text } from "@aidex/ui/primitives";
import { call } from "@aidex/client/rpc";
import { useState } from "react";
import { toAidexError } from "@aidex/client/errors";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useRpc } from "../../hooks/useRpc";
export function ModelDetailsPage() {
  const { route, navigate } = useNavigation();
  const preferences = usePreferences();
  const models = useRpc("model.list");
  const [error, setError] = useState<string>();
  const model = models.data?.models.find(
    (candidate) =>
      `${candidate.model.provider_id}/${candidate.model.model_id}` === route.id,
  );
  return (
    <PageFrame width="default">
      <PageHeader
        title={model?.display_name ?? "Model"}
        back={{ label: "Models", onClick: () => navigate({ page: "models" }) }}
      />
      {(error || models.error) && (
        <Notice tone="danger">{error ?? models.error?.message}</Notice>
      )}
      {model ? (
        <Stack gap={3}>
          <Text>
            {model.model.provider_id}/{model.model.model_id}
          </Text>
          <Text>Location: {model.locality}</Text>
          <Text>Context window: {model.context_window || "Unknown"}</Text>
          <Text>Tool calls: {model.capabilities.tool_calls}</Text>
          <Button
            onClick={() =>
              void call("model.use", model.model)
                .then(() => preferences.setModelId(route.id!))
                .catch((caught) => setError(toAidexError(caught).message))
            }
          >
            Use this model
          </Button>
        </Stack>
      ) : (
        !models.loading && (
          <Notice>
            This model is no longer registered. Reconnect its provider or start
            local inference.
          </Notice>
        )
      )}
    </PageFrame>
  );
}
