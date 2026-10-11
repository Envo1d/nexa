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
    if (normalized.includes(query) && !existing.includes(normalized)) {
      values.set(
        JSON.stringify([value.category, normalized, value.copyright ?? ""]),
        value,
      );
    }
  }
  return [...values.values()]
    .sort(
      (a, b) =>
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
