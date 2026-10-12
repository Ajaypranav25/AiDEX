"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";
import { FileCodeIcon, FileIcon } from "../../icons/development";
import { DownloadIcon, ImageIcon, LoaderIcon } from "../../icons/interface";
import { FileTextIcon } from "../../icons/objects";
import { IconButton } from "../../primitives/IconButton";
import { cx } from "../../utils/cx";
import { formatAttachmentSize } from "../../utils/format";
import { ImageLightbox } from "./ImageLightbox";
import {
  attachmentKindLabel,
  formatPageCount,
  normalizeAttachmentKind,
  safeImageDataUrl,
  type AttachmentKind,
} from "./model";
import styles from "./MessageAttachments.module.css";

export type MessageAttachmentItem = {
  id: string;
  name: string;
  /** Images show in a grid; every other kind as a row. Unknown kinds read as "File". */
  kind: AttachmentKind | (string & {});
  /** Size in bytes. */
  size?: number;
  /** PDF page count. */
  pages?: number;
  /** An image's `data:image/...` URL once loaded; other URLs are ignored. */
  src?: string;
  /**
   * `loading`: an image is being read, or a file is downloading (its button shows a spinner).
   * `error`: the image could not be loaded (its tile keeps the name).
   */
  status?: "loading" | "ready" | "error";
  /** Extra muted text for a row, e.g. "Not sent to models". */
  note?: string;
};

export type MessageAttachmentsProps = {
  items: readonly MessageAttachmentItem[];
  /**
   * Asks the app for an image's data URL: called when a tile without `src` scrolls into view or the
   * viewer shows it. It can repeat for the same id, so cache by id.
   */
  onRequestImage?: (id: string) => void;
  /** Adds a Download button to file rows (`Download <name>`). */
  onDownload?: (id: string) => void;
  /** Accessible name of the group (default "Attachments"). */
  label?: string;
  className?: string;
};

const ROW_ICON: Record<AttachmentKind, ReactNode> = {
  image: <ImageIcon />,
  pdf: <FileTextIcon />,
  text: <FileCodeIcon />,
  file: <FileIcon />,
};

/**
 * Attachments of a sent message, for a transcript bubble: images in a two-column grid of 4:3 tiles
 * (at most 210px wide; a placeholder with the name while loading or after a failure; a click opens
 * the image viewer), other files as rows with kind, name, size, and a Download button.
 */
export function MessageAttachments({
  items,
  onRequestImage,
  onDownload,
  label = "Attachments",
  className,
}: MessageAttachmentsProps) {
  const images = items.filter((item) => normalizeAttachmentKind(item.kind) === "image");
  const files = items.filter((item) => normalizeAttachmentKind(item.kind) !== "image");
  const [viewerIndex, setViewerIndex] = useState<number | null>(null);
  const tileRefs = useRef(new Map<string, HTMLElement>());
  const requestRef = useRef(onRequestImage);
  useEffect(() => {
    requestRef.current = onRequestImage;
  });

  // Ask for images as their tiles come into view; tiles that already have a source are skipped.
  const pendingKey = images
    .filter((item) => !safeImageDataUrl(item.src) && item.status !== "loading" && item.status !== "error")
    .map((item) => item.id)
    .join("\n");
  useEffect(() => {
    if (!pendingKey || !requestRef.current) return;
    const pending = new Set(pendingKey.split("\n"));
    if (typeof IntersectionObserver === "undefined") {
      pending.forEach((id) => requestRef.current?.(id));
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const id = (entry.target as HTMLElement).dataset.attachmentId;
          observer.unobserve(entry.target);
          if (id && pending.has(id)) requestRef.current?.(id);
        }
      },
      { rootMargin: "200px" },
    );
    pending.forEach((id) => {
      const tile = tileRefs.current.get(id);
      if (tile) observer.observe(tile);
    });
    return () => observer.disconnect();
  }, [pendingKey]);

  const hasImages = images.length > 0;
  useEffect(() => {
    if (!hasImages) setViewerIndex(null);
  }, [hasImages]);

  // The viewer asks for the image it shows when the grid has not asked yet. A failed image is not
  // asked again here, so a persistent failure cannot turn into a request loop.
  const viewed = viewerIndex !== null ? images[viewerIndex] : undefined;
  const viewedId =
    viewed && !safeImageDataUrl(viewed.src) && viewed.status !== "loading" && viewed.status !== "error"
      ? viewed.id
      : undefined;
  useEffect(() => {
    if (viewedId) requestRef.current?.(viewedId);
  }, [viewedId]);

  if (items.length === 0) return null;

  return (
    <div className={cx(styles.attachments, className)} role="group" aria-label={label}>
      {images.length > 0 ? (
        <ul className={cx(styles.grid, images.length === 1 && styles.single)}>
          {images.map((item, index) => {
            const src = safeImageDataUrl(item.src);
            return (
              <li
                key={item.id}
                className={styles.tile}
                data-attachment-id={item.id}
                ref={(node) => {
                  if (node) tileRefs.current.set(item.id, node);
                  else tileRefs.current.delete(item.id);
                }}
              >
                <button
                  type="button"
                  className={styles.tileButton}
                  aria-label={`Preview ${item.name}`}
                  title={item.status === "error" ? `${item.name}: image unavailable` : item.name}
                  onClick={() => setViewerIndex(index)}
                >
                  {src ? (
                    <img className={styles.image} src={src} alt="" draggable={false} />
                  ) : (
                    <span className={styles.placeholder}>
                      {item.status === "loading" ? (
                        <LoaderIcon size={16} className={styles.spinner} />
                      ) : (
                        <ImageIcon size={16} />
                      )}
                      <span className={styles.placeholderName}>{item.name}</span>
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      ) : null}
      {files.length > 0 ? (
        <ul className={styles.rows}>
          {files.map((item) => {
            const kind = normalizeAttachmentKind(item.kind);
            const facts = [
              attachmentKindLabel(kind),
              formatPageCount(item.pages),
              item.size !== undefined ? formatAttachmentSize(item.size) : "",
            ].filter(Boolean);
            const downloading = item.status === "loading";
            return (
              <li key={item.id} className={styles.row}>
                <span className={styles.rowIcon} aria-hidden="true">
                  {ROW_ICON[kind]}
                </span>
                <span className={styles.rowText}>
                  <span className={styles.rowName} title={item.name}>
                    {item.name}
                  </span>
                  <span className={styles.rowMeta}>
                    {facts.join(" · ")}
                    {item.note ? <span className={styles.note}>{` · ${item.note}`}</span> : null}
                  </span>
                </span>
                {onDownload ? (
                  <IconButton
                    size="sm"
                    label={downloading ? `Downloading ${item.name}` : `Download ${item.name}`}
                    icon={downloading ? <LoaderIcon className={styles.spinner} /> : <DownloadIcon />}
                    aria-busy={downloading || undefined}
                    disabled={downloading}
                    onClick={() => onDownload(item.id)}
                  />
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
      <ImageLightbox
        open={viewerIndex !== null && images.length > 0}
        images={images.map((item) => ({
          name: item.name,
          src: safeImageDataUrl(item.src),
          loading: item.status === "loading" || (item.status !== "error" && onRequestImage !== undefined),
        }))}
        index={Math.min(viewerIndex ?? 0, Math.max(0, images.length - 1))}
        onIndexChange={setViewerIndex}
        onClose={() => setViewerIndex(null)}
      />
    </div>
  );
}
