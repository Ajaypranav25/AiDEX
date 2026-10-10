# Encryption and identity (screen 33)

`EncryptionStep` collects the workspace owner name, a vault password with length, match, and strength checks, and shows a fixed sample recovery key with copy and download actions that only explain they are not available yet. The passwords stay in the step's local state and are discarded when the step unmounts; the wizard only learns whether they pass validation. Encryption is not implemented.
