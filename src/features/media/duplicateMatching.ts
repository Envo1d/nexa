import type { MediaItem } from "../../types/media";

export interface DuplicateFile {
  item: MediaItem;
  main: boolean;
  sha256: string;
  visual: string | null;
}

export interface DuplicatePair {
  left: DuplicateFile;
  right: DuplicateFile;
  exact: boolean;
  distance: number;
}

interface Node {
  hash: bigint;
  files: DuplicateFile[];
  children: Map<number, Node>;
}

export function hamming(left: bigint, right: bigint): number {
  let bits = left ^ right;
  let count = 0;

  while (bits) {
    bits &= bits - 1n;
    count++;
  }

  return count;
}

function comparable(left: MediaItem, right: MediaItem): boolean {
  if (!left.width || !left.height || !right.width || !right.height)
    return false;

  const ratio = left.width / left.height / (right.width / right.height);

  return ratio >= 0.85 && ratio <= 1 / 0.85;
}

/** Exact groups use one anchor; visual matches use a metric tree, never transitive grouping. */
export function duplicatePairKey(left: string, right: string): string {
  return [left, right].sort().join(":");
}

export function createDuplicateMatcher(
  limit = 1000,
  rejected = new Set<string>(),
) {
  const exact = new Map<string, DuplicateFile>();
  const pairs: DuplicatePair[] = [];
  let root: Node | undefined;

  function add(file: DuplicateFile) {
    if (pairs.length >= limit) return;

    const key = file.sha256;

    const anchor = exact.get(key);

    if (anchor) {
      pairs.push({
        left: anchor,
        right: file,
        exact: true,
        distance: 0,
      });

      return;
    }

    exact.set(key, file);

    if (!file.visual || file.item.kind !== "image") return;

    const hash = BigInt(`0x${file.visual}`);

    const pending = root ? [root] : [];

    while (pending.length && pairs.length < limit) {
      const node = pending.pop()!;

      const distance = hamming(hash, node.hash);

      if (distance <= 8) {
        for (const previous of node.files) {
          if (
            comparable(previous.item, file.item) &&
            !rejected.has(duplicatePairKey(previous.sha256, file.sha256))
          ) {
            pairs.push({
              left: previous,
              right: file,
              exact: false,
              distance,
            });
          }

          if (pairs.length >= limit) break;
        }
      }

      for (const [edge, child] of node.children) {
        if (edge >= distance - 8 && edge <= distance + 8) pending.push(child);
      }
    }

    if (!root) {
      root = {
        hash,
        files: [file],
        children: new Map(),
      };

      return;
    }

    let node = root;

    while (true) {
      const distance = hamming(hash, node.hash);

      if (!distance) {
        node.files.push(file);
        break;
      }

      const child = node.children.get(distance);

      if (child) node = child;
      else {
        node.children.set(distance, {
          hash,
          files: [file],
          children: new Map(),
        });
        break;
      }
    }
  }

  return {
    add,
    pairs,
    get full() {
      return pairs.length >= limit;
    },
  };
}
