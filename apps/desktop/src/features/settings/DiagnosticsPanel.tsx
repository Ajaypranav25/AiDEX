import { KeyValueList, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { DownloadIcon, StethoscopeIcon } from "@aidex/ui/icons";
import { Button, Stack, StatusIndicator } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { sampleDoctorChecks, sampleSystemInfo } from "./sample";

/** Doctor checks for the drive and host, system details, and log export. */
export function DiagnosticsPanel() {
  const notify = useToast();
  const warnings = sampleDoctorChecks.filter((check) => check.tone === "warning").length;

  return (
    <Stack gap={10}>
      <section>
        <SectionHeader
          title="Health checks"
          meta={`${sampleDoctorChecks.length - warnings} passed · ${warnings} warning`}
          actions={
            <Stack direction="row" gap={2}>
              <Button
                variant="outline"
                size="md"
                icon={<StethoscopeIcon />}
                onClick={() => notify("Preview: diagnostics will run when AiDEX Core is connected.")}
              >
                Run diagnostics
              </Button>
              <Button
                variant="ghost"
                size="md"
                icon={<DownloadIcon />}
                onClick={() => notify("Preview: exporting logs will be available when AiDEX Core is connected.")}
              >
                Export logs
              </Button>
            </Stack>
          }
        />
        <List>
          {sampleDoctorChecks.map((check) => (
            <ListRow
              key={check.id}
              title={check.title}
              subtitle={check.detail}
              trailing={<StatusIndicator tone={check.tone} label={check.status} />}
            />
          ))}
        </List>
      </section>
      <section>
        <SectionHeader title="System information" />
        <KeyValueList items={sampleSystemInfo} />
      </section>
    </Stack>
  );
}
