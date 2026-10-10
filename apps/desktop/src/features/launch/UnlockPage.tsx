import { useRef, useState, type FormEvent } from "react";
import { Brand, MediaItem } from "@aidex/ui/components";
import { FocusLayout } from "@aidex/ui/desktop";
import { ArrowRightIcon, HardDriveIcon, KeyIcon, LockIcon } from "@aidex/ui/icons";
import {
  Button,
  Card,
  Container,
  Divider,
  IconBadge,
  PasswordField,
  Stack,
  StatusIndicator,
  Text,
} from "@aidex/ui/primitives";
import { detectedDrive } from "../../lib/placeholders";

type Method = "password" | "recoveryKey";

type Props = { onUnlock: () => void };

const copyByMethod = {
  password: {
    noun: "password",
    FieldIcon: LockIcon,
    switchLabel: "Use recovery key",
    SwitchIcon: KeyIcon,
    autoComplete: "current-password",
  },
  recoveryKey: {
    noun: "recovery key",
    FieldIcon: KeyIcon,
    switchLabel: "Use password",
    SwitchIcon: LockIcon,
    autoComplete: "off",
  },
} as const;

export function UnlockPage({ onUnlock }: Props) {
  const [method, setMethod] = useState<Method>("password");
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string>();
  const fieldRef = useRef<HTMLInputElement>(null);
  const copy = copyByMethod[method];

  const switchMethod = () => {
    setSecret("");
    setError(undefined);
    setMethod(method === "password" ? "recoveryKey" : "password");
  };

  // No encrypted vault exists yet, so unlocking only advances the UI.
  // The secret is cleared here and is never stored, logged, or sent anywhere.
  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!secret) {
      setError(`Enter your ${copy.noun} to continue.`);
      fieldRef.current?.focus();
      return;
    }
    setSecret("");
    onUnlock();
  };

  return (
    <FocusLayout>
      <Container size="xs" verticallyCentered>
        <Stack gap={10}>
          <Stack gap={2} align="center">
            <Brand size="xl" layout="stacked" />
            <Text variant="lead" tone="secondary" align="center">
              Your encrypted AI workspace
            </Text>
          </Stack>
          <Stack gap={8}>
            <Card padding="md">
              <MediaItem
                media={<IconBadge icon={<HardDriveIcon />} />}
                title={detectedDrive.name}
                description={detectedDrive.description}
                trailing={<StatusIndicator tone="success" label="Detected" />}
              />
            </Card>
            <Card>
              <form onSubmit={handleSubmit} noValidate>
                <Stack gap={7}>
                  <MediaItem
                    media={<IconBadge tone="accent" icon={<LockIcon />} />}
                    title="Unlock your workspace"
                    titleLevel={1}
                    description={`Enter your ${copy.noun} to access your encrypted AiDEX workspace.`}
                  />
                  <Stack gap={4}>
                    <PasswordField
                      key={method}
                      ref={fieldRef}
                      label={`Workspace ${copy.noun}`}
                      placeholder={`Enter your ${copy.noun}`}
                      leadingIcon={<copy.FieldIcon />}
                      revealLabel={`Show ${copy.noun}`}
                      autoComplete={copy.autoComplete}
                      autoFocus
                      required
                      value={secret}
                      error={error}
                      onChange={(event) => {
                        setSecret(event.target.value);
                        setError(undefined);
                      }}
                    />
                    <Button type="submit" variant="tonal" size="lg" align="center" fullWidth trailing={<ArrowRightIcon />}>
                      Unlock Workspace
                    </Button>
                    <Divider label="or" />
                    <Button variant="outline" size="lg" align="center" fullWidth icon={<copy.SwitchIcon />} onClick={switchMethod}>
                      {copy.switchLabel}
                    </Button>
                  </Stack>
                </Stack>
              </form>
            </Card>
          </Stack>
        </Stack>
      </Container>
    </FocusLayout>
  );
}
