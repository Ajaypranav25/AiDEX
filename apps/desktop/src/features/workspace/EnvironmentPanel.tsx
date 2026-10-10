import { DataTable, KeyValueList, SectionHeader, type DataTableColumn } from "@aidex/ui/components";
import { RefreshIcon } from "@aidex/ui/icons";
import { Button, Stack, StatusIndicator } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import type { SampleWorkspace } from "../../lib/sample/workspaces";
import type { ProjectProfile } from "./sampleProjects";

type Toolchain = ProjectProfile["toolchains"][number];
type Props = { workspace: SampleWorkspace; profile: ProjectProfile };

const columns: readonly DataTableColumn<Toolchain>[] = [
  { key: "name", header: "Tool", rowHeader: true },
  { key: "version", header: "Version", width: "120px" },
  { key: "source", header: "Found in" },
  {
    key: "status",
    header: "Status",
    width: "140px",
    render: (tool) => <StatusIndicator tone={tool.status === "Detected" ? "success" : "warning"} label={tool.status} />,
  },
];

/** Toolchains detected in the workspace and the runtime agents use there. */
export function EnvironmentPanel({ workspace, profile }: Props) {
  const notify = useToast();

  return (
    <Stack gap={8}>
      <section aria-labelledby="environment-tools-title">
        <Stack gap={3}>
          <SectionHeader
            title={<span id="environment-tools-title">Detected toolchains</span>}
            meta={`${profile.toolchains.length} found`}
            actions={
              <Button
                variant="outline"
                size="md"
                icon={<RefreshIcon />}
                onClick={() => notify(`Preview: detecting tools in ${workspace.name} will be available when AiDEX Core is connected.`)}
              >
                Detect again
              </Button>
            }
          />
          <DataTable
            caption={`Toolchains detected in ${workspace.name}`}
            hideCaption
            columns={columns}
            rows={profile.toolchains}
            rowKey={(tool) => tool.name}
            emptyText="No toolchains detected yet."
          />
        </Stack>
      </section>
      <section aria-labelledby="environment-runtime-title">
        <Stack gap={3}>
          <SectionHeader title={<span id="environment-runtime-title">Runtime</span>} />
          <KeyValueList
            items={[
              { label: "Runtime", value: "Host process · restricted" },
              { label: "File access", value: `Limited to ${workspace.path}` },
              { label: "Network", value: "Asks before connecting" },
              { label: "Shell commands", value: "Need approval" },
            ]}
          />
        </Stack>
      </section>
    </Stack>
  );
}
