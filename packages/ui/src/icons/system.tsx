import { createIcon } from "./createIcon";

const shieldOutline = "M12 3 4.5 6v5.5c0 4.6 3.2 8.2 7.5 9.5 4.3-1.3 7.5-4.9 7.5-9.5V6Z";

export const ShieldIcon = createIcon("ShieldIcon", <path d={shieldOutline} />);

export const ShieldCheckIcon = createIcon(
  "ShieldCheckIcon",
  <>
    <path d={shieldOutline} />
    <path d="m9 12 2 2 4-4" />
  </>,
);

export const DatabaseIcon = createIcon(
  "DatabaseIcon",
  <>
    <ellipse cx="12" cy="6" rx="7.5" ry="2.75" />
    <path d="M4.5 6v12c0 1.5 3.4 2.75 7.5 2.75s7.5-1.25 7.5-2.75V6M4.5 12c0 1.5 3.4 2.75 7.5 2.75s7.5-1.25 7.5-2.75" />
  </>,
);

export const PuzzleIcon = createIcon(
  "PuzzleIcon",
  <path d="M7 6h3a2 2 0 0 1 4 0h3a1.5 1.5 0 0 1 1.5 1.5v3a2 2 0 0 1 0 4v3a1.5 1.5 0 0 1-1.5 1.5H7a1.5 1.5 0 0 1-1.5-1.5v-3a2 2 0 0 1 0-4v-3A1.5 1.5 0 0 1 7 6Z" />,
);

export const PlugIcon = createIcon(
  "PlugIcon",
  <path d="M9 3.5V8M15 3.5V8M6.5 8h11v3a5.5 5.5 0 0 1-11 0ZM12 16.5v4" />,
);

export const SparklesIcon = createIcon(
  "SparklesIcon",
  <path d="m12 3.5 1.9 5.1 5.1 1.9-5.1 1.9L12 17.5l-1.9-5.1L5 10.5l5.1-1.9ZM18.5 15.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7Z" />,
);

export const ServerIcon = createIcon(
  "ServerIcon",
  <>
    <rect x="3.5" y="4" width="17" height="7" rx="1.5" />
    <rect x="3.5" y="13" width="17" height="7" rx="1.5" />
    <path d="M7 7.5h.01M7 16.5h.01" />
  </>,
);

export const BotIcon = createIcon(
  "BotIcon",
  <>
    <rect x="4.5" y="8" width="15" height="11" rx="2.5" />
    <path d="M12 8V4.5M9.5 13h.01M14.5 13h.01M2.5 12.5v2M21.5 12.5v2" />
  </>,
);

export const GaugeIcon = createIcon(
  "GaugeIcon",
  <>
    <path d="M4.2 17.5a8.5 8.5 0 1 1 15.6 0M12 13.5l3.5-4" />
    <circle cx="12" cy="13.5" r="1.25" />
  </>,
);

export const ActivityIcon = createIcon("ActivityIcon", <path d="M3 12h4l2.5-7 5 14 2.5-7h4" />);

export const UsbDriveIcon = createIcon(
  "UsbDriveIcon",
  <>
    <rect x="7" y="9" width="10" height="12" rx="2" />
    <path d="M9 9V4h6v5M10.5 6h.01M13.5 6h.01" />
  </>,
);

export const EjectIcon = createIcon("EjectIcon", <path d="M12 5 4.5 13h15ZM5 18h14" />);

export const WifiIcon = createIcon(
  "WifiIcon",
  <path d="M5 12.5a10 10 0 0 1 14 0M8 15.5a5.5 5.5 0 0 1 8 0M2 9.5a14 14 0 0 1 20 0M12 19h.01" />,
);

export const LaptopIcon = createIcon(
  "LaptopIcon",
  <>
    <rect x="4.5" y="5" width="15" height="10.5" rx="1.5" />
    <path d="M2.5 19h19" />
  </>,
);

export const GlobeIcon = createIcon(
  "GlobeIcon",
  <>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5S9.5 6 12 3.5Z" />
  </>,
);

export const LinkIcon = createIcon(
  "LinkIcon",
  <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />,
);

export const IdCardIcon = createIcon(
  "IdCardIcon",
  <>
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <circle cx="9" cy="11" r="2" />
    <path d="M6 16c.6-1.5 1.7-2.2 3-2.2s2.4.7 3 2.2M14.5 10h4M14.5 13.5h3" />
  </>,
);

export const TerminalIcon = createIcon(
  "TerminalIcon",
  <>
    <rect x="3" y="4.5" width="18" height="15" rx="2" />
    <path d="m7 9.5 3 2.5-3 2.5M12.5 15h4.5" />
  </>,
);

export const QrCodeIcon = createIcon(
  "QrCodeIcon",
  <>
    <rect x="4" y="4" width="6" height="6" rx="1" />
    <rect x="14" y="4" width="6" height="6" rx="1" />
    <rect x="4" y="14" width="6" height="6" rx="1" />
    <path d="M14 14h2.5v2.5H14ZM18 18h2M20 14v2M14 20h2" />
  </>,
);

export const ClipboardCheckIcon = createIcon(
  "ClipboardCheckIcon",
  <>
    <rect x="5.5" y="4.5" width="13" height="16" rx="2" />
    <path d="M9 4.5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1H9ZM9 13l2 2 4-4" />
  </>,
);

export const LockOpenIcon = createIcon(
  "LockOpenIcon",
  <>
    <rect x="5" y="10.5" width="14" height="10" rx="2" />
    <path d="M8 10.5v-3a4 4 0 0 1 7.6-1.7M12 14.5v2" />
  </>,
);

export const StethoscopeIcon = createIcon(
  "StethoscopeIcon",
  <>
    <path d="M6 3.5H5v5a4 4 0 0 0 8 0v-5h-1M9 12.5v2a5 5 0 0 0 10 0v-1" />
    <circle cx="19" cy="11.5" r="2" />
  </>,
);
