/* eslint-disable @stylistic/key-spacing -- Type annotations follow type-annotation-spacing. */

type FileDetailsSource
  = | {
    kind: "comparison";
    start: string;
    end: string;
    path: string;
    previousPath: string | null;
    pathBytes?: number[] | undefined;
    previousPathBytes?: number[] | undefined;
    status: string;
  }
  | {
    kind: "worktree";
    fingerprint: string;
    path: string;
    previousPath: string | null;
    pathBytes?: number[] | null | undefined;
    previousPathBytes?: number[] | null | undefined;
  };

interface FileVersion {
  path: string;
  byteSize: number | null;
  format: string | null;
  preview: string | null;
}

interface FileDetails {
  before: FileVersion | null;
  after: FileVersion | null;
}

const bytesPerKilobyte = 1000;
const bytesPerMegabyte = bytesPerKilobyte * bytesPerKilobyte;
const sizeFormatter = new Intl.NumberFormat("en", { maximumFractionDigits: 1 });
const imageFormats = new Set(["JPEG", "PNG", "GIF", "WebP"]);

function formatSizeChange(delta: number, exact = false): string {
  const bytes = Math.abs(delta);
  const sign = delta > 0 ? "+" : delta < 0 ? "−" : "";
  if (exact) return `${ sign }${ sizeFormatter.format(bytes) } bytes`;
  if (bytes >= bytesPerMegabyte) return `${ sign }${ sizeFormatter.format(bytes / bytesPerMegabyte) } MB`;
  if (bytes >= bytesPerKilobyte) return `${ sign }${ sizeFormatter.format(bytes / bytesPerKilobyte) } KB`;
  return `${ sign }${ bytes } B`;
}

function isImageVersion(version: FileVersion | null): boolean {
  return version?.preview != null || imageFormats.has(version?.format ?? "");
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export { fileName, formatSizeChange, isImageVersion };
export type { FileDetails, FileDetailsSource, FileVersion };
