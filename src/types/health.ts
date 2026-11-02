import type { MediaItem, MediaKind } from "./media";

export type HealthCategory = "missing" | "operations" | "previews" | "metadata";

export interface HealthIssue {
  key: string;
  mediaId: string | null;
  path: string;
  kind: string;
  reason: string;
  technical: boolean;
  collection: string;
}

export interface HealthPage {
  counts: [number, number, number, number];
  total: number;
  issues: HealthIssue[];
}

export interface RelinkProposal {
  token: string;
  path: string;
  consumesDiscovery: boolean;
}

export interface HealthMedia {
  id: string;
  path: string;
  filename: string;
  relativePath: string;
  kind: MediaKind;
  extension: string;
  revision: string | null;
  availability: string;
  size: number | null;
  width: number | null;
  height: number | null;
  modifiedAt: number | null;
  createdAt: number | null;
  duration: number | null;
  fps: number | null;
  codec: string | null;
  bitrate: number | null;
  favorite: boolean;
  nativeRedistribution: boolean;
  artist: string | null;
  copyrights: string[];
  characterCopyrights: { name: string; copyright: string }[];
  tags: string[];
  videoTitle: string | null;
}

export function healthMediaItem(file: HealthMedia): MediaItem {
  return {
    ...file,
    id: `database-${file.id}`,
    nativeId: file.id,
    nativeRevision: file.revision ?? "imported",
    preview: "",
    size: file.size ?? -1,
    width: file.width ?? 0,
    height: file.height ?? 0,
    modifiedAt: file.modifiedAt ?? 0,
    createdAt: file.createdAt ?? 0,
    duration: file.duration === null ? undefined : file.duration / 1000,
    fps: file.fps ?? undefined,
    codec: file.codec ?? undefined,
    bitrate: file.bitrate ?? undefined,
    artist: file.artist ?? "",
    videoTitle: file.videoTitle ?? undefined,
    characters: [
      ...new Set(file.characterCopyrights.map((entry) => entry.name)),
    ],
  };
}
