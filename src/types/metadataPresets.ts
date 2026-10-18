export interface PresetMetadata {
  artist: string;
  copyrights: string[];
  characters: { name: string; copyright: string }[];
  tags: string[];
}

export interface MetadataPreset {
  id: string;
  name: string;
  metadata: PresetMetadata;
}
