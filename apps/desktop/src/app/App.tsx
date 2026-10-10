"use client";

import { useEffect, useState } from "react";
import { RuntimeDetectionPage } from "../features/launch/RuntimeDetectionPage";
import { SelectDrivePage } from "../features/launch/SelectDrivePage";
import { UnlockPage } from "../features/launch/UnlockPage";
import { useAidexStatus } from "../hooks/useAidexStatus";
import { AidexProvider } from "../services/AidexProvider";
import { Workspace } from "./Workspace";

type Stage = "drive" | "unlock" | "detect" | "workspace";

/** Launch flow: find the drive, check it and its vault, review host detection, then enter the workspace. */
export function App() {
  return (
    <AidexProvider>
      <Launch />
    </AidexProvider>
  );
}

function Launch() {
  const [stage, setStage] = useState<Stage>("drive");
  const { status } = useAidexStatus();
  const driveResolved = status?.drive.state === "resolved";

  // A drive found automatically (or just chosen) skips the selection screen.
  useEffect(() => {
    if (stage === "drive" && driveResolved) setStage("unlock");
  }, [stage, driveResolved]);

  if (stage === "drive") return <SelectDrivePage onSkip={() => setStage("unlock")} />;
  if (stage === "unlock") return <UnlockPage onContinue={() => setStage("detect")} />;
  if (stage === "detect") return <RuntimeDetectionPage onContinue={() => setStage("workspace")} />;
  return <Workspace />;
}
