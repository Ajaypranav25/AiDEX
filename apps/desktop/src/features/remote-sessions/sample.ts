// Sample data until AiDEX Core exposes remote pairing, trusted devices, and remote session state.
// Nothing here is paired, encrypted, or sent anywhere; the pairing code draws a decorative pattern only.

/** Route `id` the remote page reads as "a pairing session is open", so the sidebar can start one too. */
export const PAIRING_ROUTE_ID = "pairing";

export const samplePairing = {
  code: "482-913",
  expiresIn: "4:59",
  steps: [
    { title: "Open AiDEX on your phone", description: "Install the AiDEX companion app from your app store if needed." },
    { title: "Scan the code", description: "Tap Pair with desktop and point the camera at the code." },
    { title: "Confirm the short code", description: "Check that both screens show the same six digits, then approve." },
  ],
} as const;

export type TrustedDevice = {
  id: string;
  name: string;
  platform: "Android" | "iOS";
  lastSeen: string;
};

export const sampleTrustedDevices: readonly TrustedDevice[] = [
  { id: "pixel-8", name: "Pixel 8", platform: "Android", lastSeen: "Last seen 5 min ago" },
  { id: "iphone-15", name: "iPhone 15", platform: "iOS", lastSeen: "Last seen yesterday" },
];

export const sampleEncryption = [
  { label: "End-to-end encryption", value: "On" },
  { label: "Key fingerprint", value: "SHA-256 · 7F:3A:C9:12 … 9B:E4:05:D1" },
  { label: "Pairing", value: "QR code + short code" },
] as const;

export type RemotePermission = "view-only" | "can-approve";

export type RemoteSession = {
  id: string;
  device: string;
  user: string;
  /** The desktop session the phone is attached to. */
  session: string;
  started: string;
  status: "Connected" | "Idle";
  latency: string;
  permissions: readonly RemotePermission[];
};

export const sampleRemoteSessions: readonly RemoteSession[] = [
  {
    id: "rs-pixel",
    device: "Pixel 8",
    user: "J",
    session: "Fix authentication flow",
    started: "Today, 9:20 AM",
    status: "Connected",
    latency: "38 ms",
    permissions: ["view-only", "can-approve"],
  },
  {
    id: "rs-iphone",
    device: "iPhone 15",
    user: "J",
    session: "Improve agent sandbox",
    started: "Today, 8:05 AM",
    status: "Idle",
    latency: "112 ms",
    permissions: ["view-only"],
  },
];

export const permissionLabels: Record<RemotePermission, string> = {
  "view-only": "View only",
  "can-approve": "Can approve",
};
