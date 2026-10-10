import { useEffect, useRef } from "react";
import { Brand, InfoCard, MediaItem } from "@aidex/ui/components";
import { FocusLayout } from "@aidex/ui/desktop";
import { ArrowRightIcon, CheckCircleIcon, LayersIcon } from "@aidex/ui/icons";
import { Badge, Button, Card, Container, Grid, Heading, ProgressBar, Stack, Text } from "@aidex/ui/primitives";
import { hostChecks } from "../../lib/placeholders";

type Props = { onContinue: () => void };

export function RuntimeDetectionPage({ onContinue }: Props) {
  // Placeholder results are already complete; live detection will report progress per check.
  const completedChecks = hostChecks.length;
  const headingRef = useRef<HTMLHeadingElement>(null);

  // The unlock button that led here has unmounted; land focus on this screen's title.
  useEffect(() => headingRef.current?.focus(), []);

  return (
    <FocusLayout>
      <Container size="lg" verticallyCentered>
        <Stack gap={10}>
          <Stack gap={8} align="center">
            <Brand size="xl" />
            <Stack gap={3}>
              <Heading ref={headingRef} tabIndex={-1} level={1} size="display" align="center">
                Host &amp; Runtime Detection
              </Heading>
              <Text variant="lead" measure="wide" tone="secondary" align="center">
                Scanning your system to prepare the optimal AiDEX runtime.
              </Text>
            </Stack>
          </Stack>
          <Container size="md">
            <ProgressBar label="Host detection progress" value={completedChecks} max={hostChecks.length} showCount />
          </Container>
          <Grid columns={4} gap={4}>
            {hostChecks.map(({ id, label, value, detail, status, Icon }) => (
              <InfoCard
                key={id}
                icon={<Icon />}
                status={<Badge icon={<CheckCircleIcon />}>{status}</Badge>}
                label={label}
                value={value}
                detail={detail}
              />
            ))}
          </Grid>
          <Container size="md">
            <Card>
              <MediaItem
                media={<LayersIcon size={36} />}
                title="Runtime Summary"
                titleLevel={2}
                size="lg"
                description="AiDEX is ready to run with the optimal configuration for your system."
              />
            </Card>
          </Container>
          <Stack direction="row" justify="center">
            <Button variant="primary" size="xl" shape="pill" trailing={<ArrowRightIcon />} onClick={onContinue}>
              Continue to AiDEX
            </Button>
          </Stack>
        </Stack>
      </Container>
    </FocusLayout>
  );
}
