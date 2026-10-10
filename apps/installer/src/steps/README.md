# Setup steps

The six installer steps, in order. `App.tsx` owns the setup state and passes `StepProps` (`types.ts`) to each step; steps compose only `@aidex/ui`.

1. `drive-selection/DriveStep` (Drive): target drive, available space, and install folder.
2. `configuration/ConfigurationStep` (Setup): Lite, Developer, or Custom preset with an estimated size.
3. `model-selection/ModelsStep` (Models): local models, optional custom GGUF path, and the execution backend.
4. `agent-selection/AgentsStep` (Agents): external agent runtimes and extensions alongside AiDEX Native.
5. `encryption/SecurityStep` (Security): password-protected vault (the default) or an unencrypted no-password mode, with password checks.
6. `installation/InstallStep` (Install): review, a visual preview run, and its result.

Everything is a preview: drives are samples, nothing is downloaded, formatted, encrypted, or written, and the finished state never reports a real installation (`docs/requirements.md` #8).
