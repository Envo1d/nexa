import { onBeforeUnmount, ref, watch, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import type { MediaItem } from "../types/media";

type Entry = { url: string; users: number };

type Job = {
  key: string;
  id: string;
  active: () => boolean;
  done: (entry: Entry | null) => void;
};

const cache = new Map<string, Entry>();
const failed = new Set<string>();
const jobs: Job[] = [];
let running = 0;

function trim() {
  for (const [key, entry] of cache) {
    if (cache.size <= 96) break;

    if (!entry.users) {
      URL.revokeObjectURL(entry.url);
      cache.delete(key);
    }
  }

  while (failed.size > 256) failed.delete(failed.values().next().value!);
}

function pump() {
  while (running < 2 && jobs.length) {
    const job = jobs.shift()!;

    if (!job.active()) {
      job.done(null);
      continue;
    }

    const hit = cache.get(job.key);

    if (hit) {
      job.done(hit);
      continue;
    }

    running++;

    void invoke<ArrayBuffer>("get_stage_preview", { id: job.id })
      .then((bytes) => {
        let entry = cache.get(job.key);

        if (!entry) {
          entry = {
            url: URL.createObjectURL(new Blob([bytes], { type: "image/webp" })),
            users: 0,
          };
          cache.set(job.key, entry);
        }

        job.done(entry);
      })
      .catch(() => {
        failed.add(job.key);
        job.done(null);
      })
      .finally(() => {
        running--;
        trim();
        pump();
      });
  }
}

export function useNativePreview(
  item: Ref<MediaItem>,
  enabled: Ref<boolean>,
  priority: boolean,
) {
  const url = ref("");

  let current: Entry | undefined;
  let generation = 0;

  function release() {
    generation++;

    if (current) {
      current.users--;
      current = undefined;
    }

    url.value = "";
    trim();
  }

  watch(
    () => [
      item.value.nativeId,
      item.value.nativeRevision,
      item.value.path,
      item.value.availability,
      enabled.value,
    ],
    () => {
      release();

      const media = item.value;

      if (!enabled.value || !media.nativeId || media.availability === "missing")
        return;

      const key = JSON.stringify([
        media.nativeId,
        media.nativeRevision,
        media.path,
      ]);

      if (failed.has(key)) return;

      const version = generation;

      const active = () => version === generation;

      const done = (entry: Entry | null) => {
        if (!active() || !entry) return;

        current = entry;
        entry.users++;
        url.value = entry.url;
        cache.delete(key);
        cache.set(key, entry);
      };

      const hit = cache.get(key);

      if (hit) {
        done(hit);

        return;
      }

      // Remove stale queued requests before accepting fresh visible work.
      for (let index = jobs.length - 1; index >= 0; index--)
        if (!jobs[index]!.active()) jobs.splice(index, 1);

      if (jobs.length >= 64) jobs.pop()?.done(null);

      const job: Job = {
        key,
        id: media.nativeId,
        active,
        done,
      };

      if (priority) jobs.unshift(job);
      else jobs.push(job);

      pump();
    },
    { immediate: true },
  );

  onBeforeUnmount(release);

  return url;
}
