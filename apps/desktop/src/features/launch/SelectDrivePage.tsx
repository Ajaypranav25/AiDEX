import { useEffect, useRef, useState, type FormEvent } from "react";
import { Brand, MediaItem, Notice } from "@aidex/ui/components";
import { FocusLayout } from "@aidex/ui/desktop";
import { ArrowRightIcon, FolderIcon, HardDriveIcon } from "@aidex/ui/icons";
import { Button, Card, Container, Heading, IconBadge, Stack, Text, TextField } from "@aidex/ui/primitives";
import { useAidexStatus } from "../../hooks/useAidexStatus";

type Props = {
  /** Continue without a drive: every screen then runs on its labelled sample data. */
  onSkip: () => void;
};

/**
 * Shown when no drive resolved: AiDEX was not started from its drive and `AIDEX_DRIVE_ROOT` is
 * unset or unusable. The chosen folder is kept for this session only; nothing is saved on the host.
 */
export function SelectDrivePage({ onSkip }: Props) {
  const { status, error, bridgeMissing, loading, pending, actionError, selectDrive } = useAidexStatus();
  const [path, setPath] = useState("");
  const [fieldError, setFieldError] = useState<string>();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const fieldRef = useRef<HTMLInputElement>(null);

  useEffect(() => headingRef.current?.focus(), []);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!path.trim()) {
      setFieldError("Enter the folder of an AiDEX drive, for example H:\\.");
      fieldRef.current?.focus();
      return;
    }
    setFieldError(undefined);
    await selectDrive(path.trim());
  };

  const invalid = status?.drive.state === "invalid" ? status.drive : null;

  return (
    <FocusLayout>
      <Container size="sm" verticallyCentered>
        <Stack gap={10}>
          <Stack gap={6} align="center">
            <Brand size="xl" layout="stacked" />
            <Stack gap={3}>
              <Heading ref={headingRef} tabIndex={-1} level={1} size="display" align="center">
                Choose your AiDEX drive
              </Heading>
              <Text variant="lead" tone="secondary" align="center">
                AiDEX looks for the drive it runs from, then for AIDEX_DRIVE_ROOT. Neither was found.
              </Text>
            </Stack>
          </Stack>
          {loading && !status && !error ? <Notice>Looking for your AiDEX drive…</Notice> : null}
          {bridgeMissing ? (
            <Notice tone="warning">
              This window is not running inside the AiDEX app, so it cannot read drives. Continue to explore the sample
              screens.
            </Notice>
          ) : null}
          {invalid ? (
            <Notice tone="danger">
              AIDEX_DRIVE_ROOT points at {invalid.path}, which cannot be used: {invalid.error.message}
            </Notice>
          ) : null}
          {bridgeMissing || (loading && !status) ? null : (
            <Card>
              <form onSubmit={(event) => void submit(event)} noValidate>
                <Stack gap={6}>
                  <MediaItem
                    media={<IconBadge tone="accent" icon={<HardDriveIcon />} />}
                    title="Use a drive for this session"
                    titleLevel={2}
                    description="Enter the root of an initialized AiDEX drive (or a developer folder). It is remembered until you close AiDEX and is never saved on this computer."
                  />
                  <TextField
                    ref={fieldRef}
                    label="Drive folder"
                    placeholder="H:\"
                    leadingIcon={<FolderIcon />}
                    spellCheck={false}
                    autoComplete="off"
                    value={path}
                    error={fieldError ?? (actionError ? actionError.message : undefined)}
                    onChange={(event) => {
                      setPath(event.target.value);
                      setFieldError(undefined);
                    }}
                  />
                  <Button
                    type="submit"
                    variant="primary"
                    size="lg"
                    align="center"
                    fullWidth
                    disabled={pending !== null}
                    trailing={<ArrowRightIcon />}
                  >
                    {pending === "select" ? "Checking drive…" : "Use this drive"}
                  </Button>
                </Stack>
              </form>
            </Card>
          )}
          <Stack direction="row" justify="center">
            <Button variant="ghost" size="lg" onClick={onSkip}>
              Continue without a drive (sample data)
            </Button>
          </Stack>
        </Stack>
      </Container>
    </FocusLayout>
  );
}
