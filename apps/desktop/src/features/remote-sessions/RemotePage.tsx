import { Notice, PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";

/** Remote networking is outside the Windows desktop release. */
export function RemotePage() {
  return (
    <PageFrame width="default">
      <PageHeader
        title="Remote access"
        description="Connect another device to your portable workspace."
      />
      <Notice title="Coming soon" tone="info">
        Remote pairing and session control will be available in a future
        release.
      </Notice>
    </PageFrame>
  );
}
