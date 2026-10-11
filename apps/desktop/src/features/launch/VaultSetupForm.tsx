import { useRef, useState, type FormEvent } from "react";
import { MediaItem } from "@aidex/ui/components";
import { ArrowRightIcon, KeyIcon } from "@aidex/ui/icons";
import { Button, Checkbox, IconBadge, PasswordField, Stack } from "@aidex/ui/primitives";
import { MIN_PASSPHRASE_CHARS } from "@aidex/client/dto/vault";

type Props = {
  /**
   * Receives the passphrase once; both fields are cleared immediately and nothing is kept. The
   * caller passes it straight to `vault.init`.
   */
  onSubmit: (request: { passphrase: string; recovery: boolean }) => void;
  busy?: boolean;
  error?: string;
};

/** Create the drive's encrypted vault: passphrase, confirmation, and an optional one-time recovery key. */
export function VaultSetupForm({ onSubmit, busy = false, error }: Props) {
  const [passphrase, setPassphrase] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [recovery, setRecovery] = useState(true);
  const [fieldError, setFieldError] = useState<string>();
  const fieldRef = useRef<HTMLInputElement>(null);

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const problem =
      [...passphrase].length < MIN_PASSPHRASE_CHARS
        ? `Use at least ${MIN_PASSPHRASE_CHARS} characters.`
        : passphrase !== confirmation
          ? "The passphrases do not match."
          : undefined;
    if (problem) {
      setFieldError(problem);
      fieldRef.current?.focus();
      return;
    }
    const submitted = passphrase;
    setPassphrase("");
    setConfirmation("");
    onSubmit({ passphrase: submitted, recovery });
  };

  return (
    <form onSubmit={handleSubmit} noValidate>
      <Stack gap={7}>
        <MediaItem
          media={<IconBadge tone="accent" icon={<KeyIcon />} />}
          title="Create your vault"
          titleLevel={2}
          description="This drive has no vault yet. Choose a passphrase: it derives the key (Argon2id) that encrypts your secrets on the drive. AiDEX never stores it."
        />
        <Stack gap={4}>
          <PasswordField
            ref={fieldRef}
            label="Vault passphrase"
            placeholder={`At least ${MIN_PASSPHRASE_CHARS} characters`}
            revealLabel="Show passphrase"
            autoComplete="new-password"
            autoFocus
            required
            value={passphrase}
            error={fieldError ?? error}
            onChange={(event) => {
              setPassphrase(event.target.value);
              setFieldError(undefined);
            }}
          />
          <PasswordField
            label="Confirm passphrase"
            revealLabel="Show confirmation"
            autoComplete="new-password"
            required
            value={confirmation}
            onChange={(event) => {
              setConfirmation(event.target.value);
              setFieldError(undefined);
            }}
          />
          <Checkbox
            label="Create a recovery key"
            description="Shown once after the vault is created. It unlocks the vault if you forget the passphrase."
            checked={recovery}
            onChange={(event) => setRecovery(event.target.checked)}
          />
          <Button type="submit" variant="tonal" size="lg" align="center" fullWidth disabled={busy} trailing={<ArrowRightIcon />}>
            {busy ? "Creating vault…" : "Create vault"}
          </Button>
        </Stack>
      </Stack>
    </form>
  );
}
