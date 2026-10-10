import { Notice, SectionHeader, SettingRow } from "@aidex/ui/components";
import { Select, Stack, SwatchPicker, Switch } from "@aidex/ui/primitives";
import { accentPresets } from "@aidex/ui/themes";
import { usePreferences } from "../../app/providers/preferences";
export function AppearancePanel() {
  const { accent, setAccent, settings, updateSettings, error } =
    usePreferences();
  const update = (patch: Parameters<typeof updateSettings>[0]) =>
    void updateSettings(patch).catch(() => undefined);
  return (
    <Stack gap={6}>
      {error && <Notice tone="danger">{error}</Notice>}
      <SectionHeader title="Color and theme" />
      <SettingRow
        title="Accent color"
        description="Saved on the portable drive."
        control={
          <SwatchPicker
            label="Accent color"
            swatches={accentPresets}
            value={accent}
            onValueChange={setAccent}
            allowCustom
          />
        }
      />
      <SettingRow
        title="Theme"
        control={
          <Select
            label="Theme"
            value={settings.theme}
            options={[
              { value: "dark", label: "Dark" },
              { value: "light", label: "Light" },
              { value: "system", label: "Follow system" },
            ]}
            onChange={(event) =>
              update({ theme: event.target.value as typeof settings.theme })
            }
          />
        }
      />
      <SectionHeader title="Interface" />
      <SettingRow
        title="Compact density"
        description="Use tighter spacing in lists and controls."
        control={
          <Switch
            checked={settings.density === "compact"}
            onCheckedChange={(checked) =>
              update({ density: checked ? "compact" : "comfortable" })
            }
          />
        }
      />
      <SettingRow
        title="Reduce motion"
        description="Turn off interface transitions and animations."
        control={
          <Switch
            checked={settings.reduce_motion}
            onCheckedChange={(reduce_motion) => update({ reduce_motion })}
          />
        }
      />
      <SettingRow
        title="Show keyboard hints"
        control={
          <Switch
            checked={settings.keyboard_hints}
            onCheckedChange={(keyboard_hints) => update({ keyboard_hints })}
          />
        }
      />
    </Stack>
  );
}
