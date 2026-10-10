import { useState } from "react";
import { Notice, PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { Button, Select, Stack, TextField } from "@aidex/ui/primitives";
import { call } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
export function CreateWorkspacePage() {
  const { navigate } = useNavigation();
  const preferences = usePreferences();
  const [source, setSource] = useState("host");
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const create = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const workspace =
        source === "host"
          ? await call("workspace.add", {
              path: path.trim(),
              ...(name.trim() ? { name: name.trim() } : {}),
            })
          : await call("workspace.create_drive", { name: name.trim() });
      await call("workspace.select", { id_or_name: workspace.id });
      preferences.setWorkspaceId(workspace.id);
      navigate({ page: "workspace", id: workspace.id });
    } catch (caught) {
      setError(toAidexError(caught).message);
    } finally {
      setBusy(false);
    }
  };
  return (
    <PageFrame width="narrow">
      <Stack gap={4}>
        <PageHeader
          title="Add workspace"
          description="Register an existing directory or create a project on the portable drive."
        />
        {error && <Notice tone="danger">{error}</Notice>}
        <Select
          label="Location"
          value={source}
          options={[
            { value: "host", label: "Existing folder" },
            { value: "drive", label: "New portable project" },
          ]}
          onChange={(event) => setSource(event.target.value)}
        />
        {source === "host" && (
          <TextField
            label="Absolute folder path"
            value={path}
            onChange={(event) => setPath(event.target.value)}
          />
        )}
        <TextField
          label="Project name"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <Button
          disabled={busy || (source === "host" ? !path.trim() : !name.trim())}
          onClick={() => void create()}
        >
          Add workspace
        </Button>
      </Stack>
    </PageFrame>
  );
}
