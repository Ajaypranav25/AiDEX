import { PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { GitCompareIcon } from "@aidex/ui/icons";
import { Button, TabPanel, Tabs } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { tabOf } from "../../lib/routes";
import { ExecutionPanel, GraphPanel } from "./ExecutionPanels";

const TABS_ID = "execution-tabs";

const tabItems = [
  { value: "execution", label: "Execution" },
  { value: "graph", label: "Task graph" },
] as const;

export function ExecutionPage() {
  const { route, navigate, replace, back, canGoBack } = useNavigation();
  const tab = tabOf(route, "execution");

  return (
    <PageFrame width="wide">
      <PageHeader
        back={{ label: "Agent run", onClick: canGoBack ? back : () => navigate({ page: "agent" }) }}
        title="Execution & task graph"
        description="Inspect the steps behind this agent run."
        actions={
          <Button variant="outline" size="md" icon={<GitCompareIcon />} onClick={() => navigate({ page: "review" })}>
            Review changes
          </Button>
        }
      />
      <Tabs id={TABS_ID} label="Run views" variant="pill" items={tabItems} value={tab} onValueChange={(value) => replace({ ...route, tab: value })} />
      <TabPanel tabsId={TABS_ID} value={tab}>
        {tab === "graph" ? <GraphPanel /> : <ExecutionPanel />}
      </TabPanel>
    </PageFrame>
  );
}
