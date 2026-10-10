import type { Ref } from "react";
import { Meter, Notice, PageHeader } from "@aidex/ui/components";
import { ChoiceCardGroup, Grid, PasswordField, Stack, Text } from "@aidex/ui/primitives";
import type { VaultMode } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { StepProps } from "../types";
import { MIN_PASSWORD_LENGTH, passwordStrength } from "./sample";

type Props = StepProps & {
  password: string;
  confirmation: string;
  onPasswordChange: (value: string) => void;
  onConfirmationChange: (value: string) => void;
  /** Set after Continue validates the passwords; the text itself is discarded when the step closes. */
  passwordAccepted: boolean;
  error?: string;
  passwordRef: Ref<HTMLInputElement>;
};

const vaultOptions = [
  {
    value: "password",
    title: "Password-protected vault",
    description: "Require a password to unlock vault secrets. Data on the drive stays encrypted.",
    meta: "Recommended",
  },
  { value: "none", title: "No password", description: "Open AiDEX directly without an unlock prompt.", meta: "Not recommended" },
] as const;

/** Step 5: choose how the portable vault is protected and set its password. */
export function SecurityStep(props: Props) {
  const { state, update, titleRef, password, confirmation, passwordAccepted, error, passwordRef } = props;
  const strength = passwordStrength(password);
  const accepted = passwordAccepted && !password && !confirmation;

  return (
    <>
      <PageHeader
        eyebrow={stepKicker(4)}
        title="Security & identity"
        description="Choose how the portable vault that holds API keys and sign-in tokens is protected."
        titleRef={titleRef}
      />
      <ChoiceCardGroup
        label="Vault access"
        variant="rows"
        options={vaultOptions}
        value={state.vault}
        onValueChange={(vault: VaultMode) => update({ vault })}
      />
      {state.vault === "password" ? (
        <Stack gap={4}>
          <Grid columns={2} gap={3}>
            <PasswordField
              ref={passwordRef}
              label="Vault password"
              size="md"
              autoComplete="new-password"
              placeholder={`At least ${MIN_PASSWORD_LENGTH} characters`}
              value={password}
              error={error}
              required
              onChange={(event) => props.onPasswordChange(event.target.value)}
            />
            <PasswordField
              label="Confirm password"
              revealLabel="Show confirmation"
              size="md"
              autoComplete="new-password"
              placeholder="Repeat password"
              value={confirmation}
              required
              onChange={(event) => props.onConfirmationChange(event.target.value)}
            />
          </Grid>
          <Meter label="Password strength" value={strength.score} max={4} valueText={strength.label} tone={strength.tone} />
          {accepted ? (
            <Notice tone="success">Password meets the requirements. It is not kept by Setup.</Notice>
          ) : null}
          <Text tone="muted">
            Setup does not create the vault yet: it is created in AiDEX on this drive's first launch, where you set this password. Setup discards it when you leave this step.
          </Text>
        </Stack>
      ) : (
        <Notice tone="warning" variant="banner">
          No-password mode leaves the portable vault unencrypted. Avoid storing API keys or sign-in tokens on an unprotected
          drive. You can turn on password protection later.
        </Notice>
      )}
    </>
  );
}
