import { useRef, useState } from "react";
import { CodeDiff, PageHeader, SectionHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { CheckIcon, ChevronLeftIcon, ChevronRightIcon, PlayIcon, XIcon } from "@aidex/ui/icons";
import { Badge, Button, IconButton, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { useToast } from "../../app/providers/toast";
import { changedFiles, fileById, RUN_TESTS_MESSAGE, totals } from "./sample";
import { VerificationSection } from "./VerificationSection";

type Decision = "accepted" | "rejected";

export function ReviewPage() {
  const { route, replace } = useNavigation();
  const notify = useToast();
  const file = fileById(route.id);
  const index = changedFiles.indexOf(file);
  // Per-file decisions are local to this preview; applying them needs AiDEX Core.
  // Keyed by file id because switching files only changes the route, so the page doesn't remount.
  const [decisions, setDecisions] = useState<Readonly<Record<string, Decision>>>({});
  const decision = decisions[file.id] ?? null;
  // The decision swaps the file's buttons, so focus moves to whichever control replaces the clicked one.
  const undoRef = useRef<HTMLButtonElement>(null);
  const acceptFileRef = useRef<HTMLButtonElement>(null);

  const decide = (next: Decision) => {
    setDecisions((current) => ({ ...current, [file.id]: next }));
    requestAnimationFrame(() => undoRef.current?.focus());
  };
  const undoDecision = () => {
    setDecisions((current) => Object.fromEntries(Object.entries(current).filter(([id]) => id !== file.id)));
    requestAnimationFrame(() => acceptFileRef.current?.focus());
  };

  const runTests = () => notify(RUN_TESTS_MESSAGE);
  const previousRef = useRef<HTMLButtonElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);

  const showFile = (offset: number) => {
    const nextIndex = index + offset;
    const target = changedFiles[nextIndex];
    if (!target) return;
    replace({ ...route, id: target.id });
    // Reaching either end disables the clicked button, so hand focus to the opposite one.
    const atEnd = nextIndex === 0 || nextIndex === changedFiles.length - 1;
    if (atEnd) requestAnimationFrame(() => (offset > 0 ? previousRef : nextRef).current?.focus());
  };

  const header = (
    <PageHeader
      title="Review changes"
      description="Inspect the patch before applying it."
      actions={
        <>
          <Button variant="outline" size="md" icon={<PlayIcon />} onClick={runTests}>
            Run tests
          </Button>
          <Button
            variant="outline"
            size="md"
            icon={<XIcon />}
            onClick={() => notify("Preview: rejecting a patch will discard it once AiDEX Core is connected.")}
          >
            Reject
          </Button>
          <Button
            variant="primary"
            size="md"
            icon={<CheckIcon />}
            onClick={() => notify("Preview: applying patches to the workspace will be available when AiDEX Core is connected.")}
          >
            Accept
          </Button>
        </>
      }
      meta={
        <Stack direction="row" gap={4} align="center" wrap>
          <Text as="span" variant="label">
            {changedFiles.length} files changed
          </Text>
          <Badge tone="success">+{totals.additions} additions</Badge>
          <Badge tone="neutral">−{totals.deletions} deletions</Badge>
          <StatusIndicator label="12 tests passed" tone="success" />
          <StatusIndicator tone="neutral" label="Sample data until AiDEX Core is connected" />
        </Stack>
      }
    />
  );

  const fileActions = decision ? (
    <>
      <Badge tone={decision === "accepted" ? "success" : "neutral"} icon={decision === "accepted" ? <CheckIcon /> : <XIcon />}>
        {decision === "accepted" ? "Marked to accept" : "Marked to reject"}
      </Badge>
      <Button ref={undoRef} variant="ghost" size="md" onClick={undoDecision}>
        Undo
      </Button>
    </>
  ) : (
    <>
      <Button variant="ghost" size="md" icon={<XIcon />} onClick={() => decide("rejected")}>
        Reject file
      </Button>
      <Button ref={acceptFileRef} variant="outline" size="md" icon={<CheckIcon />} onClick={() => decide("accepted")}>
        Accept file
      </Button>
    </>
  );

  return (
    <PageFrame>
      {header}
      <section>
        <Stack gap={4}>
          <SectionHeader
            title="Changes"
            meta={`File ${index + 1} of ${changedFiles.length}`}
            actions={
              <Stack direction="row" gap={1}>
                <IconButton
                  ref={previousRef}
                  label="Previous file"
                  icon={<ChevronLeftIcon />}
                  disabled={index === 0}
                  onClick={() => showFile(-1)}
                />
                <IconButton
                  ref={nextRef}
                  label="Next file"
                  icon={<ChevronRightIcon />}
                  disabled={index === changedFiles.length - 1}
                  onClick={() => showFile(1)}
                />
              </Stack>
            }
          />
          <CodeDiff fileName={file.path} oldCode={file.oldCode} newCode={file.newCode} actions={fileActions} />
        </Stack>
      </section>
      <VerificationSection onRunTests={runTests} />
    </PageFrame>
  );
}
