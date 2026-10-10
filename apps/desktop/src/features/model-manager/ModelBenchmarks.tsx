import { BarChart, DataTable, SectionHeader, StatTile, type DataTableColumn } from "@aidex/ui/components";
import { CpuIcon, DownloadIcon, FileTextIcon, GaugeIcon, MemoryIcon } from "@aidex/ui/icons";
import { Badge, Button, Card, Grid } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { formatGb, formatRate } from "./format";
import type { LocalModel, Quantization } from "./sample";

type Props = { model: LocalModel };

const RATE_UNIT = "tok/s";

/** Headline resource and speed figures for the installed quantization. */
export function ModelStats({ model }: Props) {
  const { stats } = model;
  return (
    <Grid columns={4} gap={4}>
      <StatTile label="RAM" value={formatGb(stats.ramGb)} hint="Peak while generating" icon={<MemoryIcon />} />
      <StatTile
        label="VRAM"
        value={stats.vramGb > 0 ? formatGb(stats.vramGb) : "None"}
        hint={stats.vramGb > 0 ? "Full GPU offload" : "Runs on the CPU"}
        icon={<CpuIcon />}
      />
      <StatTile
        label="Tokens per second"
        value={formatRate(stats.tokensPerSecond)}
        hint={`${model.quantization} · best available backend`}
        icon={<GaugeIcon />}
        tone="accent"
      />
      <StatTile label="Context length" value={stats.context} hint={stats.contextHint} icon={<FileTextIcon />} />
    </Grid>
  );
}

/** Speed charts by quantization and by hardware backend. */
export function BenchmarkCharts({ model }: Props) {
  return (
    <section aria-labelledby="benchmarks-heading">
      <SectionHeader title={<span id="benchmarks-heading">Benchmarks</span>} meta="Sample results · 512-token prompt" />
      <Grid columns={2} gap={4}>
        <Card padding="md">
          <BarChart
            caption="Tokens per second by quantization"
            unit={RATE_UNIT}
            data={model.quantizations.map((quant) => ({
              label: quant.installed ? `${quant.name} (installed)` : quant.name,
              value: quant.tokensPerSecond,
              highlight: quant.installed,
            }))}
          />
        </Card>
        <Card padding="md">
          <BarChart
            caption={`Tokens per second by hardware · ${model.quantization}`}
            unit={RATE_UNIT}
            data={model.hardware.map((entry, index) => ({
              label: entry.label,
              value: entry.tokensPerSecond,
              highlight: index === 0,
            }))}
          />
        </Card>
      </Grid>
    </section>
  );
}

/** Available quantizations of this model with their trade-offs. */
export function QuantizationTable({ model }: Props) {
  const notify = useToast();

  const columns: readonly DataTableColumn<Quantization>[] = [
    { key: "name", header: "Quantization", rowHeader: true },
    { key: "size", header: "Size", align: "end", render: (row) => formatGb(row.sizeGb) },
    { key: "ram", header: "RAM", align: "end", render: (row) => formatGb(row.ramGb) },
    { key: "quality", header: "Quality" },
    { key: "speed", header: "Speed", align: "end", render: (row) => `${formatRate(row.tokensPerSecond)} ${RATE_UNIT}` },
    {
      key: "action",
      header: "Action",
      align: "end",
      width: "150px",
      render: (row) =>
        row.installed ? (
          <Badge tone="success">Installed</Badge>
        ) : (
          <Button
            variant="outline"
            icon={<DownloadIcon />}
            aria-label={`Download ${model.name} ${row.name}`}
            onClick={() =>
              notify(`Preview: downloading ${model.name} ${row.name} will be available when AiDEX Core is connected.`)
            }
          >
            Download
          </Button>
        ),
    },
  ];

  return (
    <section aria-labelledby="quantizations-heading">
      <SectionHeader title={<span id="quantizations-heading">Quantizations</span>} meta={`${model.quantizations.length} variants`} />
      <DataTable
        caption={`Quantizations of ${model.name}`}
        hideCaption
        columns={columns}
        rows={model.quantizations}
        rowKey={(row) => row.name}
      />
    </section>
  );
}
