export interface MetadataSuggestion {
  name: string;
  category: string;
  copyright?: string;
}

export interface MetadataCatalog {
  copyrights: string[];
  artists: string[];
  tags: string[];
  characters: { name: string; copyright: string }[];
}

export function nameSearchRank(name: string, search: string): number {
  const position = name
    .toLocaleLowerCase()
    .indexOf(search.trim().toLocaleLowerCase());

  return position < 0 ? 2 : position === 0 ? 0 : 1;
}

export function searchByName<T extends { name: string }>(
  values: readonly T[],
  search: string,
): T[] {
  return values
    .map((value) => ({ value, rank: nameSearchRank(value.name, search) }))
    .filter(({ rank }) => rank < 2)
    .sort((a, b) => a.rank - b.rank)
    .map(({ value }) => value);
}

export function metadataSearchRank(names: string[], terms: string[]): number {
  let rank = 0;

  for (const term of terms) {
    const termRank = names.reduce(
      (best, name) => Math.min(best, nameSearchRank(name, term)),
      2,
    );

    rank = Math.max(rank, termRank);
  }

  return rank;
}

export function normalizeMetadataSearch(search: string): string {
  return search
    .split(";")
    .map((value) => value.trim())
    .filter(Boolean)
    .join("; ");
}

export function chooseMetadataSuggestion(search: string, name: string): string {
  const parts = search.split(";");

  parts.pop();

  return normalizeMetadataSearch([...parts, name].join("; "));
}

export function metadataSuggestions(
  search: string,
  entries: MetadataSuggestion[],
): MetadataSuggestion[] {
  const parts = search.split(";");

  const query = parts.pop()?.trim().toLocaleLowerCase() ?? "";

  if (!query) return [];

  const existing = parts.map((value) => value.trim().toLocaleLowerCase());

  const values = new Map<string, MetadataSuggestion>();

  for (const value of entries) {
    const normalized = value.name.toLocaleLowerCase();

    if (
      nameSearchRank(value.name, query) < 2 &&
      !existing.includes(normalized)
    ) {
      values.set(
        JSON.stringify([value.category, normalized, value.copyright ?? ""]),
        value,
      );
    }
  }

  return [...values.values()]
    .sort(
      (a, b) =>
        nameSearchRank(a.name, query) - nameSearchRank(b.name, query) ||
        a.name.localeCompare(b.name) ||
        (a.copyright ?? "").localeCompare(b.copyright ?? ""),
    )
    .slice(0, 30);
}

export function catalogSuggestions(
  catalog?: MetadataCatalog,
): MetadataSuggestion[] {
  if (!catalog) return [];

  return [
    ...catalog.artists.map((name) => ({ name, category: "Artist" })),
    ...catalog.copyrights.map((name) => ({ name, category: "Copyright" })),
    ...catalog.tags.map((name) => ({ name, category: "Tag" })),
    ...catalog.characters.map((value) => ({ ...value, category: "Character" })),
  ];
}
