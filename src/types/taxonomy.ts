export type TaxonomyCategory = "copyright" | "artist" | "tag" | "character";

export interface TaxonomyEntry {
  id: string;
  name: string;
  count: number;
  copyright?: string;
}

export interface TaxonomyCatalog {
  copyrights: TaxonomyEntry[];
  artists: TaxonomyEntry[];
  tags: TaxonomyEntry[];
  characters: TaxonomyEntry[];
}

export const categoryKey = {
  copyright: "copyrights",
  artist: "artists",
  tag: "tags",
  character: "characters",
} as const;
