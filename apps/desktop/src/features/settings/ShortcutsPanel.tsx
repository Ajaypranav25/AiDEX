import { DataTable, SectionHeader, type DataTableColumn } from "@aidex/ui/components";
import { Kbd, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { useModifierKeyLabel } from "../../hooks/useModifierKeyLabel";
import { sampleShortcuts, type Shortcut } from "./sample";

/** Keyboard shortcuts, marking which ones this build already handles. */
export function ShortcutsPanel() {
  const modifierKey = useModifierKeyLabel();

  const keysFor = (shortcut: Shortcut) => [modifierKey, ...(shortcut.shift ? ["Shift"] : []), shortcut.key];

  const columns: readonly DataTableColumn<Shortcut>[] = [
    { key: "action", header: "Action", rowHeader: true },
    {
      key: "keys",
      header: "Keys",
      render: (shortcut) => (
        <Stack direction="row" gap={1} align="center">
          {keysFor(shortcut).map((key) => (
            <Kbd key={key}>{key}</Kbd>
          ))}
        </Stack>
      ),
    },
    {
      key: "status",
      header: "Status",
      render: (shortcut) => (
        <StatusIndicator tone={shortcut.wired ? "success" : "neutral"} label={shortcut.wired ? "Works today" : "Planned"} />
      ),
    },
  ];

  return (
    <section>
      <SectionHeader title="Keyboard shortcuts" meta={`${sampleShortcuts.filter((s) => s.wired).length} of ${sampleShortcuts.length} active`} />
      <Stack gap={4}>
        <DataTable caption="Keyboard shortcuts" hideCaption columns={columns} rows={sampleShortcuts} rowKey={(row) => row.id} />
        <Text tone="muted">Planned shortcuts are listed so you can learn them now; they start working as each screen is wired up.</Text>
      </Stack>
    </section>
  );
}
