import { List, ListRow, SectionHeader } from "@aidex/ui/components";
import { CloudIcon, GlobeIcon, LockIcon } from "@aidex/ui/icons";
import { Button, IconBadge, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { providerAccounts } from "./sample";

/** Providers tab of the Model Center: optional cloud accounts and their connection state. */
export function ProvidersPanel() {
  const notify = useToast();
  const connectedCount = providerAccounts.filter((account) => account.connected).length;

  return (
    <Stack gap={6}>
      <section aria-labelledby="provider-accounts-heading">
        <SectionHeader
          title={<span id="provider-accounts-heading">Provider accounts</span>}
          meta={`${connectedCount} of ${providerAccounts.length} connected`}
        />
        <List metaWidth="140px">
          {providerAccounts.map((account) => (
            <ListRow
              key={account.id}
              media={<IconBadge size="md" icon={account.id === "compatible" ? <GlobeIcon /> : <CloudIcon />} />}
              title={account.name}
              subtitle={account.description}
              meta={[
                <StatusIndicator
                  key="status"
                  tone={account.connected ? "success" : "neutral"}
                  label={account.connected ? "Connected" : "Not connected"}
                />,
              ]}
              trailing={
                <Button
                  variant={account.connected ? "ghost" : "outline"}
                  aria-label={`${account.connected ? "Manage" : "Connect"} ${account.name}`}
                  onClick={() =>
                    notify(
                      account.connected
                        ? `Preview: managing the ${account.name} account will be available when AiDEX Core is connected.`
                        : `Preview: connecting ${account.name} will be available when AiDEX Core is connected.`,
                    )
                  }
                >
                  {account.connected ? "Manage" : "Connect"}
                </Button>
              }
            />
          ))}
        </List>
      </section>
      <Stack direction="row" gap={2} align="center">
        <LockIcon size={16} />
        <Text tone="muted">Credentials are stored in the encrypted AiDEX vault and never included in prompts.</Text>
      </Stack>
    </Stack>
  );
}
