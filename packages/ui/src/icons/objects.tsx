import { createIcon } from "./createIcon";

/** Filled tile with punched-out dots; used to mark a primary project. */
export const GridTileIcon = createIcon(
  "GridTileIcon",
  <path
    fill="currentColor"
    stroke="none"
    fillRule="evenodd"
    d="M7 3h10a4 4 0 0 1 4 4v10a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4V7a4 4 0 0 1 4-4ZM11.1 9a2.1 2.1 0 1 1-4.2 0 2.1 2.1 0 1 1 4.2 0ZM17.1 9a2.1 2.1 0 1 1-4.2 0 2.1 2.1 0 1 1 4.2 0ZM11.1 15a2.1 2.1 0 1 1-4.2 0 2.1 2.1 0 1 1 4.2 0ZM17.1 15a2.1 2.1 0 1 1-4.2 0 2.1 2.1 0 1 1 4.2 0Z"
  />,
);

export const LayersIcon = createIcon(
  "LayersIcon",
  <>
    <path d="m12 3 9 4.5-9 4.5-9-4.5Z" />
    <path d="m3 12 9 4.5 9-4.5" />
    <path d="m3 16.5 9 4.5 9-4.5" />
  </>,
);

export const BracesIcon = createIcon(
  "BracesIcon",
  <>
    <path d="M8.5 3.5h-.75A2.25 2.25 0 0 0 5.5 5.75V9.5A2.5 2.5 0 0 1 3 12a2.5 2.5 0 0 1 2.5 2.5v3.75a2.25 2.25 0 0 0 2.25 2.25h.75" />
    <path d="M15.5 3.5h.75a2.25 2.25 0 0 1 2.25 2.25V9.5A2.5 2.5 0 0 0 21 12a2.5 2.5 0 0 0-2.5 2.5v3.75a2.25 2.25 0 0 1-2.25 2.25h-.75" />
  </>,
);

export const FolderIcon = createIcon(
  "FolderIcon",
  <path d="M3.5 7a2 2 0 0 1 2-2h3.6a2 2 0 0 1 1.5.7l1.3 1.6h6.6a2 2 0 0 1 2 2v8.2a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z" />,
);

export const CircleDollarIcon = createIcon(
  "CircleDollarIcon",
  <>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M14.75 9.25c-.4-.9-1.45-1.5-2.75-1.5-1.65 0-2.75.85-2.75 2.05 0 2.9 5.5 1.5 5.5 4.35 0 1.2-1.15 2.1-2.75 2.1-1.35 0-2.4-.6-2.8-1.55M12 6.25v1.5M12 16.25v1.5" />
  </>,
);

export const FileTextIcon = createIcon(
  "FileTextIcon",
  <>
    <path d="M14 3.5H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5Z" />
    <path d="M14 3.5v5h5M9 13h6M9 16.5h6M9 9.5h2" />
  </>,
);

export const CodeIcon = createIcon(
  "CodeIcon",
  <path d="m8.5 7-5 5 5 5M15.5 7l5 5-5 5M13.5 5l-3 14" />,
);

export const LightbulbIcon = createIcon(
  "LightbulbIcon",
  <>
    <path d="M9.5 18.5h5M10.5 21h3" />
    <path d="M12 3a6 6 0 0 0-3.6 10.8c.6.45 1.1 1.15 1.1 1.95V16h5v-.25c0-.8.5-1.5 1.1-1.95A6 6 0 0 0 12 3Z" />
  </>,
);

export const HardDriveIcon = createIcon(
  "HardDriveIcon",
  <>
    <path d="m4.5 13.5 2.1-7.9A1.5 1.5 0 0 1 8.05 4.5h7.9a1.5 1.5 0 0 1 1.45 1.1l2.1 7.9" />
    <rect x="3.5" y="13.5" width="17" height="6" rx="1.5" />
    <path d="M7.5 16.5h3" />
  </>,
);

export const LockIcon = createIcon(
  "LockIcon",
  <>
    <rect x="5" y="10.5" width="14" height="10" rx="2" />
    <path d="M8 10.5v-3a4 4 0 0 1 8 0v3M12 14.5v2" />
  </>,
);

export const KeyIcon = createIcon(
  "KeyIcon",
  <>
    <circle cx="7.5" cy="16.5" r="4" />
    <path d="m10.4 13.6 9.1-9.1M16 8l2.5 2.5M13.5 10.5l2 2" />
  </>,
);

export const CpuIcon = createIcon(
  "CpuIcon",
  <>
    <rect x="6" y="6" width="12" height="12" rx="2" />
    <path d="M9.5 2.5V6M14.5 2.5V6M9.5 18v3.5M14.5 18v3.5M2.5 9.5H6M2.5 14.5H6M18 9.5h3.5M18 14.5h3.5" />
  </>,
);

export const MonitorIcon = createIcon(
  "MonitorIcon",
  <>
    <rect x="3" y="4" width="18" height="12.5" rx="2" />
    <path d="M8.5 20.5h7M12 16.5v4" />
  </>,
);

export const MemoryIcon = createIcon(
  "MemoryIcon",
  <>
    <rect x="2.5" y="6.5" width="19" height="10" rx="1.5" />
    <path d="M8.5 9.5v4M12 9.5v4M15.5 9.5v4M6 16.5V19M10 16.5V19M14 16.5V19M18 16.5V19" />
  </>,
);

export const MessageSquareIcon = createIcon(
  "MessageSquareIcon",
  <path d="M20.5 15a2 2 0 0 1-2 2H8l-4.5 3.5V6a2 2 0 0 1 2-2h13a2 2 0 0 1 2 2Z" />,
);
