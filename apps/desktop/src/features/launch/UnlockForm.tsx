import { useRef, useState, type FormEvent } from "react";
import { MediaItem } from "@aidex/ui/components";
import { ArrowRightIcon, KeyIcon, LockIcon } from "@aidex/ui/icons";
import { Button, Divider, IconBadge, PasswordField, Stack } from "@aidex/ui/primitives";

export type UnlockMethod = "password" | "recoveryKey";

type Props = {
  /**
   * Receives the secret once; the field is cleared immediately and the secret is never kept,
   * logged, or rendered. `UnlockPage` passes it straight to `vault.unlock`.
   */
  onSubmit: (credential: { method: UnlockMethod; secret: string }) => void;
  /** Set while an unlock is being verified. */
  busy?: boolean;
  /** Why the last attempt failed (for example a wrong password). */
  error?: string;
};

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

/** Passphrase or recovery-key entry for a locked vault (`vault.status` reports `locked`). */
export function UnlockForm({ onSubmit, busy = false, error }: Props) {
  const [method, setMethod] = useState<UnlockMethod>("password");
  const [secret, setSecret] = useState("");
  const [fieldError, setFieldError] = useState<string>();
  const fieldRef = useRef<HTMLInputElement>(null);
  const copy = copyByMethod[method];

  const switchMethod = () => {
    setSecret("");
    setFieldError(undefined);
    setMethod(method === "password" ? "recoveryKey" : "password");
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!secret) {
      setFieldError(`Enter your ${copy.noun} to continue.`);
      fieldRef.current?.focus();
      return;
    }
    const submitted = secret;
    setSecret("");
    onSubmit({ method, secret: submitted });
  };

  return (
    <form onSubmit={handleSubmit} noValidate>
      <Stack gap={7}>
        <MediaItem
          media={<IconBadge tone="accent" icon={<LockIcon />} />}
          title="Unlock your workspace"
          titleLevel={2}
          description={`Enter your vault ${copy.noun} to unlock the secrets on this drive.`}
        />
        <Stack gap={4}>
          <PasswordField
            key={method}
            ref={fieldRef}
            label={`Workspace ${copy.noun}`}
            labelVisibility="hidden"
            placeholder={`Enter your ${copy.noun}`}
            leadingIcon={<copy.FieldIcon />}
            revealLabel={`Show ${copy.noun}`}
            autoComplete={copy.autoComplete}
            autoFocus
            required
            value={secret}
            error={fieldError ?? error}
            onChange={(event) => {
              setSecret(event.target.value);
              setFieldError(undefined);
            }}
          />
          <Button type="submit" variant="tonal" size="lg" align="center" fullWidth disabled={busy} trailing={<ArrowRightIcon />}>
            {busy ? "Unlocking…" : "Unlock Workspace"}
          </Button>
          <Divider label="or" />
          <Button variant="outline" size="lg" align="center" fullWidth icon={<copy.SwitchIcon />} onClick={switchMethod}>
            {copy.switchLabel}
          </Button>
        </Stack>
      </Stack>
    </form>
  );
}
