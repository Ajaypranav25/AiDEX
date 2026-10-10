import { useEffect, useState } from "react";
import { CodeBlock, Meter, SectionHeader } from "@aidex/ui/components";
import { CopyIcon, DownloadIcon } from "@aidex/ui/icons";
import { Checkbox, IconButton, PasswordField, Stack, Text, TextField } from "@aidex/ui/primitives";
import type { Notify } from "../../lib/useToasts";
import { MIN_PASSWORD_LENGTH, MIN_STRENGTH_SCORE, passwordStrength, SAMPLE_RECOVERY_KEY } from "./sample";

type Props = {
  ownerName: string;
  onOwnerNameChange: (name: string) => void;
  recoveryKeySaved: boolean;
  onRecoveryKeySavedChange: (saved: boolean) => void;
  /** Reports whether the password fields pass validation; the passwords themselves never leave this step. */
  onPasswordReadyChange: (ready: boolean) => void;
  notify: Notify;
};

function passwordProblem(longEnough: boolean, strongEnough: boolean): string | undefined {
  if (!longEnough) return `Use at least ${MIN_PASSWORD_LENGTH} characters.`;
  if (!strongEnough) return "Too weak. Make it longer or mix upper and lower case, digits, and symbols.";
  return undefined;
}

/** Screen 33: owner name, vault password with strength and match checks, and the recovery key. */
export function EncryptionStep({ ownerName, onOwnerNameChange, recoveryKeySaved, onRecoveryKeySavedChange, onPasswordReadyChange, notify }: Props) {
  // Passwords live only in this component's state and are discarded when the step unmounts.
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [touched, setTouched] = useState({ password: false, confirmation: false });
  const strength = passwordStrength(password);
  const longEnough = password.length >= MIN_PASSWORD_LENGTH;
  const strongEnough = strength.score >= MIN_STRENGTH_SCORE;
  const matches = confirmation === password;
  const ready = longEnough && strongEnough && matches;

  useEffect(() => {
    onPasswordReadyChange(ready);
    return () => onPasswordReadyChange(false);
  }, [ready, onPasswordReadyChange]);

  const passwordError = touched.password ? passwordProblem(longEnough, strongEnough) : undefined;
  const confirmationError = touched.confirmation && confirmation && !matches ? "Passwords do not match." : undefined;

  return (
    <Stack gap={8}>
      <section>
        <Stack gap={4}>
          <SectionHeader title="Identity" />
          <TextField
            label="Workspace owner"
            hint="Shown on the unlock screen. Stored on the drive only."
            autoComplete="name"
            value={ownerName}
            onChange={(event) => onOwnerNameChange(event.currentTarget.value)}
          />
        </Stack>
      </section>
      <section>
        <Stack gap={4}>
          <SectionHeader title="Vault password" />
          <PasswordField
            label="Vault password"
            hint={`At least ${MIN_PASSWORD_LENGTH} characters and rated Fair or better. A passphrase of several words works well.`}
            autoComplete="new-password"
            value={password}
            error={passwordError}
            onChange={(event) => setPassword(event.currentTarget.value)}
            onBlur={() => setTouched((current) => ({ ...current, password: true }))}
          />
          <Meter label="Password strength" value={strength.score} max={4} valueText={strength.label} tone={strength.tone} />
          <PasswordField
            label="Confirm password"
            revealLabel="Show confirmation"
            autoComplete="new-password"
            value={confirmation}
            error={confirmationError}
            onChange={(event) => setConfirmation(event.currentTarget.value)}
            onBlur={() => setTouched((current) => ({ ...current, confirmation: true }))}
          />
          <Text tone="muted">For your safety, both fields are cleared when you leave this step.</Text>
        </Stack>
      </section>
      <section>
        <Stack gap={4}>
          <SectionHeader title="Recovery key" />
          <Text tone="secondary">
            If you forget the password, this key is the only way to open the vault. In this preview it is a fixed sample that protects nothing.
          </Text>
          <CodeBlock
            title="Recovery key (sample)"
            code={SAMPLE_RECOVERY_KEY}
            actions={
              <>
                <IconButton
                  label="Copy recovery key"
                  icon={<CopyIcon />}
                  onClick={() => notify("Preview: nothing was copied. A real key will be generated when the vault is created.")}
                />
                <IconButton
                  label="Download recovery key"
                  icon={<DownloadIcon />}
                  onClick={() => notify("Preview: no file was saved. Downloading the key will be available with vault setup.")}
                />
              </>
            }
          />
          <Checkbox
            label="I saved my recovery key somewhere safe"
            description="Required to continue. Keep it off the drive, for example in a password manager or on paper."
            checked={recoveryKeySaved}
            onChange={(event) => onRecoveryKeySavedChange(event.currentTarget.checked)}
          />
        </Stack>
      </section>
    </Stack>
  );
}
