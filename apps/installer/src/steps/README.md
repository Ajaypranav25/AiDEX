# Setup steps

The six installer steps, in order. `App.tsx` owns the setup state and passes `StepProps` (`types.ts`) to each step; steps compose only `@aidex/ui`.

1. `drive-selection/DriveStep` (Drive): real volumes from `list_drives`; only removable drives and USB SSDs that are not the system drive can be chosen.
2. `configuration/ConfigurationStep` (Setup): Lite, Developer, or Custom preset with an estimated size against the drive's real free space.
3. `model-selection/ModelsStep` (Models): local models, optional custom GGUF path, and the execution backend.
4. `agent-selection/AgentsStep` (Agents): external agent runtimes and extensions alongside AiDEX Native.
5. `encryption/SecurityStep` (Security): password-protected vault (the default) or an unencrypted no-password mode, with password checks.
6. `installation/InstallStep` (Install): review, then a real drive initialization (inspect, `init_drive`, verify) with step-by-step progress; components without a backend are listed as not available yet.

Drive discovery and initialization are real; nothing is ever formatted, repartitioned, or deleted. Models, agents, extensions, and the bundle are sample choices whose installation is not part of this build, and the finished state reports only what actually happened (`docs/requirements.md` #8).
