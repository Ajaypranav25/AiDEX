# Launch feature

Owns the screens shown before the workspace: `UnlockPage` (drive status and password or recovery-key unlock) and `RuntimeDetectionPage` (host hardware and runtime readiness). `../../app/App.tsx` steps through them in order.

Both are UI previews. No encrypted vault exists yet, so unlocking verifies nothing: the entered secret is cleared on submit and never stored, logged, or sent. The drive and detection results are static placeholders from `../../lib/placeholders.ts` until Core exposes drive and host detection.
