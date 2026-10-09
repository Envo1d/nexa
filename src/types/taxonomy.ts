export type TaxonomyCategory = "copyright" | "artist" | "tag";
export interface TaxonomyEntry {
  id: string;
  name: string;
  count: number;
}
export interface TaxonomyCatalog {
  copyrights: TaxonomyEntry[];
  artists: TaxonomyEntry[];
  tags: TaxonomyEntry[];
}
export const categoryKey = {
  copyright: "copyrights",
  artist: "artists",
  tag: "tags",
} as const;
