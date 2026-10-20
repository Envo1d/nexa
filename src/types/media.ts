export type MediaKind = "video" | "image";

export interface CollectionPosition {
  collectionKey: string;
  mode: "grid" | "list";
  scrollTop: number;
  loaded: number;
  width: number;
  height: number;
  remSize: number;
}

export interface MediaItem {
  nativeId?: string;
  nativeRevision?: string;
  availability?: string;
  nativeRedistribution?: boolean;
  id: string;
  filename: string;
  path: string;
  kind: MediaKind;
  extension: string;
  title?: string;
  preview: string;
  modifiedAt: number;
  createdAt: number;
  size: number;
  width: number;
  height: number;
  duration?: number;
  fps?: number;
  codec?: string;
  bitrate?: number;
  favorite: boolean;
  copyrights: string[];
  artist: string;
  artists?: string[];
  characters: string[];
  characterCopyrights?: { name: string; copyright: string }[];
  videoTitle?: string;
  tags: string[];
}

export interface MediaMetadataDraft {
  copyrights: string[];
  artists: string[];
  characters: { name: string; copyright: string }[];
  tags: string[];
  videoTitle: string;
}

export function formatDate(timestamp: number): string {
  const date = new Date(timestamp);

  const pad = (value: number) => String(value).padStart(2, "0");

  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function formatDuration(seconds: number, full = false): string {
  seconds = Math.max(0, Math.floor(seconds));

  const hours = Math.floor(seconds / 3600);

  const minutes = Math.floor(seconds / 60) % 60;

  const pad = (value: number) => String(value).padStart(2, "0");

  return `${full || hours ? `${pad(hours)}:` : ""}${pad(minutes)}:${pad(seconds % 60)}`;
}

export function formatSize(bytes: number): string {
  if (bytes < 0) return "Unavailable";

  return bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(2)} GB`
    : `${(bytes / 1024 ** 2).toFixed(2)} MB`;
}
