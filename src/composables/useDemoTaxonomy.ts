import { computed, watch, type Ref } from "vue";

import type { MediaItem } from "../types/media";

import {
  categoryKey,
  type TaxonomyCategory,
  type TaxonomyCatalog,
  type TaxonomyEntry,
} from "../types/taxonomy";

export function useDemoTaxonomy(
  library: Ref<MediaItem[]>,
  stage: Ref<MediaItem[]>,
) {
  const characters = new Map<string, { name: string; copyright: string }>();

  const characterId = (name: string, copyright: string) =>
    JSON.stringify([copyright, name]);

  const qualified = (item: MediaItem) =>
    item.characterCopyrights ??
    item.characters.map((name) => ({
      name,
      copyright: item.copyrights[0] ?? "",
    }));

  const dictionaries = {
    copyrights: new Set<string>(),
    artists: new Set<string>(),
    tags: new Set<string>(),
    characters: new Set<string>(),
  };

  const names = (item: MediaItem, category: TaxonomyCategory) =>
    category === "artist"
      ? (item.artists ?? (item.artist ? [item.artist] : []))
      : item[categoryKey[category]];

  watch(
    [library, stage],
    () => {
      for (const item of [...library.value, ...stage.value]) {
        for (const entry of qualified(item))
          characters.set(characterId(entry.name, entry.copyright), entry);

        for (const category of ["copyright", "artist", "tag"] as const)
          for (const name of names(item, category))
            dictionaries[categoryKey[category]].add(name);
      }
    },
    { deep: true, immediate: true },
  );

  const catalog = computed<TaxonomyCatalog>(() => {
    const result: TaxonomyCatalog = {
      copyrights: [],
      artists: [],
      tags: [],
      characters: [],
    };

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

    result.characters = [...characters]
      .map(([id, entry]): TaxonomyEntry => ({
        id,
        ...entry,
        count: [...library.value, ...stage.value].filter((item) =>
          qualified(item).some(
            (value) =>
              value.name === entry.name && value.copyright === entry.copyright,
          ),
        ).length,
      }))
      .sort(
        (a, b) =>
          a.name.localeCompare(b.name) ||
          (a.copyright ?? "").localeCompare(b.copyright ?? ""),
      );

    return result;
  });

  function change(
    category: TaxonomyCategory,
    old: string,
    replacement?: string,
    copyright?: string,
    replacementCopyright?: string,
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

    if (category === "character") {
      if (copyright === undefined)
        throw new Error("Select a copyright-qualified character.");

      const targetCopyright = replacementCopyright ?? copyright;

      if (next && !dictionaries.copyrights.has(targetCopyright))
        throw new Error("Select the character’s copyright");

      if (
        [...characters.values()].some(
          (entry) =>
            next &&
            entry.copyright === targetCopyright &&
            !(entry.name === old && entry.copyright === copyright) &&
            entry.name.normalize("NFC").toLocaleLowerCase() ===
              next.normalize("NFC").toLocaleLowerCase(),
        )
      )
        throw new Error("This character already exists in the copyright.");

      for (const item of [...library.value, ...stage.value]) {
        const assignments = qualified(item);

        const linked = assignments.some(
          (entry) => entry.name === old && entry.copyright === copyright,
        );

        if (next && linked && !item.copyrights.includes(targetCopyright))
          item.copyrights = [...item.copyrights, targetCopyright].sort((a, b) =>
            a.localeCompare(b),
          );

        item.characterCopyrights = assignments.flatMap((entry) =>
          entry.name === old && entry.copyright === copyright
            ? next
              ? [
                  {
                    ...entry,
                    name: next,
                    copyright: targetCopyright,
                  },
                ]
              : []
            : [entry],
        );

        const removePreviousCopyright =
          next &&
          linked &&
          targetCopyright !== copyright &&
          !item.characterCopyrights.some(
            (entry) => entry.copyright === copyright,
          );

        if (removePreviousCopyright)
          item.copyrights = item.copyrights.filter(
            (value) => value !== copyright,
          );

        item.characters = [
          ...new Set(item.characterCopyrights.map((entry) => entry.name)),
        ];
      }

      characters.delete(characterId(old, copyright));

      if (next)
        characters.set(characterId(next, targetCopyright), {
          name: next,
          copyright: targetCopyright,
        });

      library.value = [...library.value];

      return 0;
    }

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

    if (category === "copyright") {
      for (const [id, entry] of [...characters]) {
        if (entry.copyright !== old) continue;

        characters.delete(id);

        if (next)
          characters.set(characterId(entry.name, next), {
            ...entry,
            copyright: next,
          });
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
