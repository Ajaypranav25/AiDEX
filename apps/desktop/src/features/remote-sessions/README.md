# Remote sessions feature

Pairing phones with this desktop and controlling which of them can see or act on running sessions. Route: `remote`, tabs `devices` and `sessions`.

- `RemotePage` — header with the "Pair device" action and underline tabs.
- `DevicesPanel` — the open pairing card (`PairingCard`: decorative QR pattern, short code, expiry, steps, cancel), trusted devices with "Revoke", the "No devices paired" empty state, and encryption details.
- `SessionsPanel` — connection status (local listener, relay, encryption) and the active remote sessions table with "Disconnect" and "Revoke".
- `RemoteSidebar` — paired devices, pair device, and active sessions, all driven by the route.

An open pairing session is part of the route (`{ page: "remote", tab: "devices", id: "pairing" }`), so both the page header and the sidebar can start it, and the sidebar highlights it.

Everything is a UI preview over `sample.ts`. The QR pattern encodes nothing and cannot be scanned. Revoking or disconnecting only removes the row locally and shows a "Preview" toast; real pairing, key exchange, and revocation belong to AiDEX Core once it exposes remote session state.
