import { createIcon } from "./createIcon";

export const GitBranchIcon = createIcon(
  "GitBranchIcon",
  <>
    <circle cx="6.5" cy="5.5" r="2" />
    <circle cx="6.5" cy="18.5" r="2" />
    <circle cx="17.5" cy="7.5" r="2" />
    <path d="M6.5 7.5v9M17.5 9.5c0 4-4 4.5-9 7" />
  </>,
);

export const GitCommitIcon = createIcon(
  "GitCommitIcon",
  <>
    <circle cx="12" cy="12" r="3.5" />
    <path d="M3 12h5.5M15.5 12H21" />
  </>,
);

export const GitCompareIcon = createIcon(
  "GitCompareIcon",
  <>
    <circle cx="6" cy="6" r="2.5" />
    <circle cx="18" cy="18" r="2.5" />
    <path d="M6 8.5V15a3 3 0 0 0 3 3h4.5M11 15.5l2.5 2.5-2.5 2.5M18 15.5V9a3 3 0 0 0-3-3h-4.5M13 3.5 10.5 6 13 8.5" />
  </>,
);

const fileOutline = "M14 3.5H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5ZM14 3.5v5h5";

export const FileIcon = createIcon("FileIcon", <path d={fileOutline} />);

export const FileCodeIcon = createIcon(
  "FileCodeIcon",
  <>
    <path d={fileOutline} />
    <path d="m10 13-2 2 2 2M14 13l2 2-2 2" />
  </>,
);

export const FolderPlusIcon = createIcon(
  "FolderPlusIcon",
  <path d="M3.5 7a2 2 0 0 1 2-2h3.6a2 2 0 0 1 1.5.7l1.3 1.6h6.6a2 2 0 0 1 2 2v8.2a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2ZM12 10.5v5M9.5 13h5" />,
);

export const HistoryIcon = createIcon(
  "HistoryIcon",
  <path d="M4 12a8 8 0 1 0 2.35-5.65L4 8.5M4 4v4.5h4.5M12 8v4l2.5 2.5" />,
);

export const ZapIcon = createIcon("ZapIcon", <path d="M13 3 5 13.5h6.5L11 21l8-10.5h-6.5Z" />);

export const FlaskIcon = createIcon(
  "FlaskIcon",
  <path d="M9.5 3.5h5M10.5 3.5v6L5.2 18.2a1.5 1.5 0 0 0 1.3 2.3h11a1.5 1.5 0 0 0 1.3-2.3L13.5 9.5v-6M7.5 15h9" />,
);
