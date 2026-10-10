import { EntityCard, KeyValueList, List, ListRow, MediaItem, Meter, SectionHeader } from "@aidex/ui/components";
import { BoxIcon, CloudIcon, DownloadIcon, SettingsIcon } from "@aidex/ui/icons";
import { Badge, Button, Card, Grid, IconBadge, Split, Stack, StatusIndicator } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { compatibilityTone, currentModelName, formatGb } from "./format";
import { downloadableModels, driveCapacityGb, localModels, runtimeInfo } from "./sample";

const usedGb = Math.round(localModels.reduce((sum, model) => sum + model.sizeGb, 0) * 10) / 10;

function CurrentModelCard() {
  const { modelId } = usePreferences();
  const { navigate } = useNavigation();
  const local = localModels.find((model) => model.id === modelId);

  return (
    <Card padding="md">
      <MediaItem
        media={<IconBadge tone="accent" icon={local ? <BoxIcon /> : <CloudIcon />} />}
        title={currentModelName(modelId)}
        description={
          local
            ? `In use for new chats and agent runs · ${local.quantization} · ${formatGb(local.sizeGb)}`
            : "In use for new chats and agent runs · runs on a connected cloud provider"
        }
        trailing={
          local ? (
            <Button variant="outline" onClick={() => navigate({ page: "model", id: local.id })}>
              View details
            </Button>
          ) : (
            <StatusIndicator tone="success" label="In use" />
          )
        }
      />
    </Card>
  );
}

function InstalledModels() {
  const { modelId } = usePreferences();
  const { navigate } = useNavigation();

  return (
    <section aria-labelledby="local-models-heading">
      <SectionHeader title={<span id="local-models-heading">On this drive</span>} meta={`${localModels.length} models`} />
      <List metaWidth="132px">
        {localModels.map((model) => {
          const inUse = model.id === modelId;
          return (
            <ListRow
              key={model.id}
              media={<IconBadge size="md" icon={<BoxIcon />} />}
              title={model.name}
              subtitle={`${model.quantization} · GGUF · ${model.summary}`}
              meta={[
                inUse ? <Badge key="use">In use</Badge> : model.kind === "embedding" ? "Embeddings" : "Chat",
                <Badge key="fit" tone={compatibilityTone(model.compatibility)}>
                  {model.compatibility}
                </Badge>,
                formatGb(model.sizeGb),
              ]}
              selected={inUse}
              onSelect={() => navigate({ page: "model", id: model.id })}
            />
          );
        })}
      </List>
    </section>
  );
}

function DownloadableModels() {
  const notify = useToast();
  return (
    <section aria-labelledby="download-models-heading">
      <SectionHeader title={<span id="download-models-heading">Available to download</span>} meta="Sample compatibility results" />
      <Grid columns={3} gap={4}>
        {downloadableModels.map((model) => (
          <EntityCard
            key={model.id}
            media={<IconBadge size="md" icon={<BoxIcon />} />}
            title={model.name}
            subtitle={model.detail}
            badges={<Badge tone={compatibilityTone(model.compatibility)}>{model.compatibility}</Badge>}
            description={model.description}
            footer={
              <Button
                variant="outline"
                icon={<DownloadIcon />}
                aria-label={`Download ${model.name}`}
                onClick={() => notify(`Preview: downloading ${model.name} will be available when AiDEX Core is connected.`)}
              >
                Download
              </Button>
            }
          />
        ))}
      </Grid>
    </section>
  );
}

function RuntimeAndStorage() {
  const { navigate } = useNavigation();
  return (
    <Split emphasis="equal">
      <section aria-labelledby="runtime-heading">
        <SectionHeader
          title={<span id="runtime-heading">Runtime</span>}
          actions={
            <Button variant="ghost" icon={<SettingsIcon />} onClick={() => navigate({ page: "settings" })}>
              Runtime settings
            </Button>
          }
        />
        <KeyValueList items={runtimeInfo} />
      </section>
      <section aria-labelledby="storage-heading">
        <SectionHeader title={<span id="storage-heading">Storage</span>} />
        <Meter
          label="Model storage on the AiDEX drive"
          value={usedGb}
          max={driveCapacityGb}
          tone="accent"
          valueText={`Models use ${usedGb} GB of 1 TB`}
        />
      </section>
    </Split>
  );
}

/** Local tab of the Model Center: current model, installed GGUF models, downloads, runtime, and storage. */
export function LocalModelsPanel() {
  return (
    <Stack gap={10}>
      <CurrentModelCard />
      <InstalledModels />
      <DownloadableModels />
      <RuntimeAndStorage />
    </Stack>
  );
}
