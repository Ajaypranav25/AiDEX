import { PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { TabPanel, Tabs } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { tabOf, type PageTab } from "../../lib/routes";
import { LocalModelsPanel } from "./LocalModelsPanel";
import { ProvidersPanel } from "./ProvidersPanel";
const items = [
  { value: "local", label: "Local models" },
  { value: "providers", label: "Provider accounts" },
] as const satisfies readonly { value: PageTab<"models">; label: string }[];
export function ModelCenterPage() {
  const { route, replace } = useNavigation();
  const tab = tabOf(route, "models");
  return (
    <PageFrame width="default">
      <PageHeader
        title="Models"
        description="Manage cloud accounts and verified inference runtimes on this drive."
      />
      <Tabs
        id="models-tabs"
        label="Model sections"
        variant="underline"
        items={items}
        value={tab}
        onValueChange={(next) => replace({ page: "models", tab: next })}
      />
      <TabPanel tabsId="models-tabs" value={tab}>
        {tab === "local" ? <LocalModelsPanel /> : <ProvidersPanel />}
      </TabPanel>
    </PageFrame>
  );
}
