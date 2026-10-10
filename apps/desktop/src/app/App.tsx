"use client";

import { useState } from "react";
import { RuntimeDetectionPage } from "../features/launch/RuntimeDetectionPage";
import { UnlockPage } from "../features/launch/UnlockPage";
import { Workspace } from "./Workspace";

type Stage = "unlock" | "detect" | "workspace";

/** Launch flow: unlock the drive, review host detection, then enter the workspace. */
export function App() {
  const [stage, setStage] = useState<Stage>("unlock");

  if (stage === "unlock") return <UnlockPage onUnlock={() => setStage("detect")} />;
  if (stage === "detect") return <RuntimeDetectionPage onContinue={() => setStage("workspace")} />;
  return <Workspace />;
}
