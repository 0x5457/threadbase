import prettyBytes from "pretty-bytes";
import type { AttachmentDto } from "../api/types/AttachmentDto";

/** Must match the server's inline-preview whitelist. */
const PREVIEWABLE_IMAGES = new Set([
  "image/png",
  "image/jpeg",
  "image/jpg",
  "image/webp",
  "image/gif",
]);

const TEXT_EXTENSIONS = new Set([
  "txt",
  "md",
  "markdown",
  "json",
  "jsonl",
  "yaml",
  "yml",
  "toml",
  "csv",
  "tsv",
  "log",
  "xml",
  "html",
  "css",
  "js",
  "jsx",
  "ts",
  "tsx",
  "py",
  "rs",
  "go",
  "java",
  "c",
  "h",
  "cpp",
  "hpp",
  "sh",
  "bash",
  "zsh",
  "sql",
  "ini",
  "conf",
  "env",
  "diff",
  "patch",
]);

export function isPreviewableImage(mimeType: string): boolean {
  return PREVIEWABLE_IMAGES.has(mimeType.trim().toLowerCase());
}

export function imageMimeForFile(file: File): string | null {
  if (isPreviewableImage(file.type)) return file.type;
  const extension = file.name.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "webp":
      return "image/webp";
    case "gif":
      return "image/gif";
    default:
      return null;
  }
}

export function isTextLike(mimeType: string, fileName?: string | null): boolean {
  const mime = mimeType.toLowerCase();
  if (mime.startsWith("text/")) return true;
  if (
    [
      "application/json",
      "application/xml",
      "application/yaml",
      "application/x-yaml",
      "application/toml",
      "application/javascript",
      "application/typescript",
      "application/x-sh",
      "application/sql",
    ].includes(mime)
  ) {
    return true;
  }
  const extension = fileName?.split(".").pop()?.toLowerCase();
  return extension ? TEXT_EXTENSIONS.has(extension) : false;
}

export function attachmentName(
  attachment: Pick<AttachmentDto, "fileName" | "mimeType">,
  fallback = "attachment",
): string {
  if (attachment.fileName) return attachment.fileName;
  return isPreviewableImage(attachment.mimeType) ? "image" : fallback;
}

export function attachmentSize(bytes: number): string {
  return prettyBytes(Math.max(0, bytes));
}
