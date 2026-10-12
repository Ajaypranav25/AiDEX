// Pure attachment helpers shared by the composer, the tray, the transcript and the drop zone.
// No DOM access beyond the types, so they run during prerendering and in unit tests.

/** What an attachment holds. The daemon decides it by sniffing bytes; anything else reads as a file. */
export type AttachmentKind = "image" | "pdf" | "text" | "file";

/** Where files entered the composer. */
export type FileIntakeSource = "picker" | "paste" | "drop";

const KIND_LABEL: Record<AttachmentKind, string> = {
  image: "Image",
  pdf: "PDF",
  text: "Text",
  file: "File",
};

/** Own keys only, so names such as `constructor` count as unknown kinds. */
function isAttachmentKind(kind: string): kind is AttachmentKind {
  return Object.prototype.hasOwnProperty.call(KIND_LABEL, kind);
}

/** Known kind, or `file` for anything else (a newer daemon may add kinds). */
export function normalizeAttachmentKind(kind: string): AttachmentKind {
  return isAttachmentKind(kind) ? kind : "file";
}

/** "Image", "PDF", "Text"; unknown kinds read as "File". */
export function attachmentKindLabel(kind: string): string {
  return KIND_LABEL[normalizeAttachmentKind(kind)];
}

/** "1 page", "12 pages"; empty for a missing or invalid count. */
export function formatPageCount(pages: number | undefined): string {
  if (pages === undefined || !Number.isFinite(pages) || pages < 1) return "";
  const whole = Math.floor(pages);
  return whole === 1 ? "1 page" : `${whole} pages`;
}

/** True when a drag carries files from outside the page (`DataTransfer.types` lists "Files"). */
export function isFileDrag(types: ArrayLike<string> | null | undefined): boolean {
  if (!types) return false;
  for (let index = 0; index < types.length; index += 1) {
    if (types[index] === "Files") return true;
  }
  return false;
}

/** An image by its declared MIME type (the daemon re-checks the bytes). */
export function isImageFile(file: { type: string }): boolean {
  return file.type.toLowerCase().startsWith("image/");
}

/**
 * Whether pasted files should become attachments instead of the default paste: any image claims the
 * paste; other files only when the clipboard carries no plain text (so copying rich text that also
 * offers a file keeps pasting text).
 */
export function shouldClaimFilePaste(files: ReadonlyArray<{ type: string }>, hasPlainText: boolean): boolean {
  if (files.length === 0) return false;
  if (files.some(isImageFile)) return true;
  return !hasPlainText;
}

/** Default size from which a plain-text paste is offered to `onLargePaste` (32 KiB). */
export const LARGE_PASTE_BYTES = 32 * 1024;

/** UTF-8 length of a string without allocating an encoded copy. */
export function utf8ByteLength(text: string): number {
  let bytes = 0;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff && index + 1 < text.length) {
      const next = text.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else bytes += 3;
    } else bytes += 3;
  }
  return bytes;
}

/** True when the text is at least `thresholdBytes` long in characters or UTF-8 bytes. */
export function isLargePaste(text: string, thresholdBytes: number = LARGE_PASTE_BYTES): boolean {
  if (text.length === 0) return false;
  if (text.length >= thresholdBytes) return true;
  // UTF-8 never takes more than three bytes per UTF-16 unit, so short text can skip the count.
  if (text.length * 3 < thresholdBytes) return false;
  return utf8ByteLength(text) >= thresholdBytes;
}

/**
 * The source only when it is an inline `data:image/...` URL. Previews never load `blob:`, file or
 * network URLs: the release CSP allows `data:` images only, and a preview must not fetch anything.
 */
export function safeImageDataUrl(src: string | null | undefined): string | undefined {
  if (typeof src !== "string") return undefined;
  return /^data:image\/[a-z0-9.+-]+[;,]/i.test(src) ? src : undefined;
}

/** Fraction from 0 to 1 as a whole percentage, clamped; invalid input gives 0. */
export function progressPercent(progress: number | undefined): number {
  if (progress === undefined || !Number.isFinite(progress)) return 0;
  return Math.round(Math.min(1, Math.max(0, progress)) * 100);
}
