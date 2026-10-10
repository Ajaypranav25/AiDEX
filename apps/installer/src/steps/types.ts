import type { Ref } from "react";
import type { VolumeInfo } from "@aidex/client/dto/drive";
import type { SetupState } from "../lib/setup";
import type { Notify } from "../lib/useToasts";

/** Props every setup step receives from `App`, which owns the setup state. */
export type StepProps = {
  state: SetupState;
  update: (patch: Partial<SetupState>) => void;
  notify: Notify;
  /** Attached to the step's heading so focus can move there when the step opens. */
  titleRef: Ref<HTMLHeadingElement>;
  /** The chosen volume from `list_drives`, or `null` before one is chosen. */
  volume: VolumeInfo | null;
};
