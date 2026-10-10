import { PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { PlusIcon } from "@aidex/ui/icons";
import { Button, StatusIndicator, TabPanel, Tabs } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { useToast } from "../../app/providers/toast";
import { tabOf, type PageTab } from "../../lib/routes";
import { LocalModelsPanel } from "./LocalModelsPanel";
import { ProvidersPanel } from "./ProvidersPanel";

type ModelsTab = PageTab<"models">;

const TABS_ID = "models-tabs";

const tabItems = [
  { value: "local", label: "Local models" },
  { value: "providers", label: "Provider accounts" },
] as const satisfies readonly { value: ModelsTab; label: string }[];

const primaryAction: Record<ModelsTab, { label: string; message: string }> = {
  local: { label: "Import model", message: "Preview: importing GGUF models will be available when AiDEX Core is connected." },
  providers: { label: "Add account", message: "Preview: adding provider accounts will be available when AiDEX Core is connected." },
};

export function ModelCenterPage() {
  const { route, replace } = useNavigation();
  const notify = useToast();
  const tab = tabOf(route, "models");
  const action = primaryAction[tab];

  return (
    <PageFrame width="default">
      <PageHeader
        title="Models"
        description="Local models and inference runtimes on the portable drive, plus optional cloud providers."
        meta={<StatusIndicator tone="neutral" label="Sample data until AiDEX Core is connected" />}
        actions={
          <Button variant="primary" icon={<PlusIcon />} onClick={() => notify(action.message)}>
            {action.label}
          </Button>
        }
      />
      <Tabs
        id={TABS_ID}
        label="Model sections"
        variant="underline"
        items={tabItems}
        value={tab}
        onValueChange={(next) => replace({ page: "models", tab: next })}
      />
      <TabPanel tabsId={TABS_ID} value={tab}>
        {tab === "local" ? <LocalModelsPanel /> : <ProvidersPanel />}
      </TabPanel>
    </PageFrame>
  );
}
