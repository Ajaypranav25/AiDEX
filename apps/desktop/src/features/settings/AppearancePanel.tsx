import { SectionHeader, SettingRow } from "@aidex/ui/components";
import { Badge, Stack, SwatchPicker, Switch } from "@aidex/ui/primitives";
import { accentPresets } from "@aidex/ui/themes";
import { usePreferences } from "../../app/providers/preferences";

// The interface switches are planned but not wired to anything yet, so they render disabled at their defaults.
const plannedSetting = () => undefined;

/** Accent (applied live), theme, and the planned interface preferences. */
export function AppearancePanel() {
  const { accent, setAccent } = usePreferences();

  return (
    <Stack gap={10}>
      <section>
        <SectionHeader title="Color and theme" />
        <SettingRow
          title="Accent color"
          description="Electric blue by default; applies only to highlights and actions. Changes apply immediately."
          control={
            <SwatchPicker label="Accent color" swatches={accentPresets} value={accent} onValueChange={setAccent} allowCustom />
          }
        />
        <SettingRow
          title="Theme"
          description="Minimal dark interface. A light theme is planned."
          control={<Badge tone="neutral">Dark</Badge>}
        />
      </section>
      <section>
        <SectionHeader title="Interface" />
        <SettingRow
          title="Compact density"
          description="Planned: tighter rows and spacing in lists and sidebars."
          controlId="setting-density"
          control={
            <Switch
              id="setting-density"
              aria-describedby="setting-density-description"
              checked={false}
              disabled
              onCheckedChange={plannedSetting}
            />
          }
        />
        <SettingRow
          title="Reduce motion"
          description="Planned: turn off transitions and animated progress."
          controlId="setting-motion"
          control={
            <Switch
              id="setting-motion"
              aria-describedby="setting-motion-description"
              checked={false}
              disabled
              onCheckedChange={plannedSetting}
            />
          }
        />
        <SettingRow
          title="Show keyboard hints"
          description="Planned: choose whether shortcut keys appear beside actions such as New session."
          controlId="setting-hints"
          control={
            <Switch
              id="setting-hints"
              aria-describedby="setting-hints-description"
              checked
              disabled
              onCheckedChange={plannedSetting}
            />
          }
        />
      </section>
    </Stack>
  );
}
