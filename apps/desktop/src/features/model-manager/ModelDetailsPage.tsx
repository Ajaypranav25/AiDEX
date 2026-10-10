import { InlineCode, KeyValueList, PageHeader, SectionHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { CheckIcon, GaugeIcon } from "@aidex/ui/icons";
import { Badge, Button, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { BenchmarkCharts, ModelStats, QuantizationTable } from "./ModelBenchmarks";
import { formatGb } from "./format";
import { localModelById, type LocalModel } from "./sample";

function detailItems(model: LocalModel) {
  const { details } = model;
  return [
    { label: "Architecture", value: details.architecture },
    { label: "Parameters", value: details.parameters },
    { label: "License", value: details.license },
    { label: "Source", value: details.source },
    { label: "SHA-256 checksum", value: <InlineCode>{details.checksum}</InlineCode> },
    { label: "Added", value: details.added },
  ];
}

export function ModelDetailsPage() {
  const { route, navigate, back, canGoBack } = useNavigation();
  const { modelId, setModelId } = usePreferences();
  const notify = useToast();
  const model = localModelById(route.id);
  const inUse = model.id === modelId;
  const isChatModel = model.kind === "chat";

  const useModel = () => {
    // aria-disabled (not disabled) keeps focus on the button once its label switches to "In use".
    if (inUse) return;
    if (!isChatModel) {
      notify(`Preview: choosing the retrieval model will be available when AiDEX Core is connected.`);
      return;
    }
    setModelId(model.id);
    notify(`New chats and agent runs will use ${model.name}.`, "success");
  };

  return (
    <PageFrame width="default">
      <PageHeader
        back={{ label: "Models", onClick: () => (canGoBack ? back() : navigate({ page: "models", tab: "local" })) }}
        title={model.name}
        description={model.summary}
        meta={
          <Stack direction="row" gap={2} align="center" wrap>
            <Badge tone="neutral">GGUF</Badge>
            <Badge tone="neutral">{model.quantization}</Badge>
            <Badge tone="neutral">{formatGb(model.sizeGb)}</Badge>
            {isChatModel ? null : <Badge tone="neutral">Embeddings</Badge>}
            {inUse ? <Badge tone="success">In use</Badge> : null}
          </Stack>
        }
        actions={
          <>
            <Button
              variant="outline"
              icon={<GaugeIcon />}
              onClick={() => notify(`Preview: benchmarking ${model.name} will be available when AiDEX Core is connected.`)}
            >
              Run benchmark
            </Button>
            <Button variant="primary" icon={<CheckIcon />} aria-disabled={inUse} onClick={useModel}>
              {inUse ? "In use" : isChatModel ? "Use this model" : "Use for retrieval"}
            </Button>
          </>
        }
      />
      <ModelStats model={model} />
      <BenchmarkCharts model={model} />
      <QuantizationTable model={model} />
      <section aria-labelledby="model-details-heading">
        <SectionHeader title={<span id="model-details-heading">Details</span>} />
        <KeyValueList items={detailItems(model)} />
      </section>
    </PageFrame>
  );
}
