import { computed, watch, type Ref } from "vue";
import type { MediaItem } from "../types/media";
import {
  categoryKey,
  type TaxonomyCategory,
  type TaxonomyCatalog,
} from "../types/taxonomy";

export function useDemoTaxonomy(
  library: Ref<MediaItem[]>,
  stage: Ref<MediaItem[]>,
) {
  const dictionaries = {
    copyrights: new Set<string>(),
    artists: new Set<string>(),
    tags: new Set<string>(),
  };
  const names = (item: MediaItem, category: TaxonomyCategory) =>
    category === "artist"
      ? (item.artists ?? (item.artist ? [item.artist] : []))
      : item[categoryKey[category]];
  watch(
    [library, stage],
    () => {
      for (const item of [...library.value, ...stage.value]) {
        for (const category of ["copyright", "artist", "tag"] as const)
          for (const name of names(item, category))
            dictionaries[categoryKey[category]].add(name);
      }
    },
    { deep: true, immediate: true },
  );
  const catalog = computed<TaxonomyCatalog>(() => {
    const result: TaxonomyCatalog = { copyrights: [], artists: [], tags: [] };
    for (const category of ["copyright", "artist", "tag"] as const) {
      const key = categoryKey[category];
      const items = [...library.value, ...stage.value];
      // Read refs to refresh counts even if the dictionary itself has no new entries.
      result[key] = [...dictionaries[key]]
        .map((name) => ({
          id: name,
          name,
          count: items.filter((item) => names(item, category).includes(name))
            .length,
        }))
        .sort((a, b) => a.name.localeCompare(b.name));
    }
    return result;
  });
  function change(
    category: TaxonomyCategory,
    old: string,
    replacement?: string,
  ) {
    const key = categoryKey[category];
    const next = replacement?.trim();
    if (
      replacement !== undefined &&
      (!next ||
        [...next].some(
          (character) =>
            character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
        ))
    )
      throw new Error("Enter a nonempty name without control characters.");
    if (
      next &&
      [...dictionaries[key]].some(
        (name) =>
          name !== old &&
          name.normalize("NFC").toLocaleLowerCase() ===
            next.normalize("NFC").toLocaleLowerCase(),
      )
    )
      throw new Error("This name already exists in the category.");
    const affected = library.value.filter((item) =>
      names(item, category).includes(old),
    );
    for (const item of [...library.value, ...stage.value]) {
      if (category === "artist") {
        item.artists = names(item, category).flatMap((name) =>
          name === old ? (next ? [next] : []) : [name],
        );
        item.artist = item.artists.join(", ");
      } else if (category === "tag") {
        item.tags = item.tags.flatMap((name) =>
          name === old ? (next ? [next] : []) : [name],
        );
      } else {
        const hadCopyright = item.copyrights.includes(old);
        item.copyrights = item.copyrights.flatMap((name) =>
          name === old ? (next ? [next] : []) : [name],
        );
        if (item.characterCopyrights) {
          item.characterCopyrights = item.characterCopyrights.flatMap(
            (entry) =>
              entry.copyright === old
                ? next
                  ? [{ ...entry, copyright: next }]
                  : []
                : [entry],
          );
          item.characters = [
            ...new Set(item.characterCopyrights.map((entry) => entry.name)),
          ];
        } else if (hadCopyright && !next) {
          // Old fixtures do not carry qualified character identity.
          item.characters = [];
        }
      }
    }
    dictionaries[key].delete(old);
    if (next) dictionaries[key].add(next);
    if (!next && category !== "tag") {
      const ids = new Set(affected.map((item) => item.id));
      library.value = library.value.filter((item) => !ids.has(item.id));
      for (const item of affected)
        if (!stage.value.some((entry) => entry.id === item.id))
          stage.value.push(item);
    } else {
      // Invalidate the computed dictionary after renaming unused entries too.
      library.value = [...library.value];
    }
    return !next && category !== "tag" ? affected.length : 0;
  }
  return { catalog, change };
}
