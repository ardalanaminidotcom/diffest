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

export type { FileDetails, FileDetailsSource, FileVersion };
