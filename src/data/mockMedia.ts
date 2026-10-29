import type { MediaItem } from "../types/media";

// Offline thumbnails rasterized once from the preserved SVG references.
const scenes = [
  { name: "Alpine_Lake", tags: ["Nature", "Mountains"] },
  {
    name: "City_Sunset",
    title: "Above the city",
    tags: ["Drone", "City", "Sunset"],
  },
  { name: "Hidden_Waterfall", tags: ["Nature", "Water"] },
  { name: "Coastal_Evening", tags: ["Travel", "Sunset"] },
  {
    name: "Ocean_View",
    title: "A moment of stillness",
    tags: ["Nature", "Sea"],
  },
  { name: "City_Lights", tags: ["City", "Night"] },
  { name: "Mountains", tags: ["Nature", "Mountains", "Travel"] },
  {
    name: "Forest_Walk",
    title: "Golden hour in the forest",
    tags: ["Nature", "Forest"],
  },
  { name: "Lake_View", tags: ["Nature", "Lake"] },
  { name: "Desert", tags: ["Travel", "Summer"] },
  {
    name: "Timelapse_City",
    title: "The city never sleeps",
    tags: ["City", "Timelapse"],
  },
  {
    name: "Northern_Lights",
    title: "Under the northern lights",
    tags: ["Nature", "Night"],
  },
];

const previews = scenes.map(
  (_, index) =>
    new URL(`../assets/previews/scene-${index}.webp`, import.meta.url).href,
);

const characterGroups = [
  ["Alice"],
  ["Alice", "Bob"],
  ["Alice", "Bob", "Carol"],
  ["Luna"],
  ["Luna", "Kai"],
  ["Luna", "Kai", "Nova"],
];

const artists = ["Mira", "Sora", "Aster", "Yuki"];

export function createMockMedia(): MediaItem[] {
  return Array.from({ length: 480 }, (_, index) => {
    const scene = index % scenes.length;
    const source = scenes[scene]!;
    const kind = [1, 4, 7, 10, 11].includes(scene) ? "video" : "image";
    const extension = kind === "video" ? "MP4" : scene === 8 ? "PNG" : "JPG";
    const groupIndex =
      (index + Math.floor(index / scenes.length)) % characterGroups.length;
    const characters = [...characterGroups[groupIndex]!];
    const artist = `${artists[Math.floor(index / scenes.length) % artists.length]} ${String(index + 1).padStart(3, "0")}`;
    const videoTitle = source.title ?? source.name.replace(/_/g, " ");
    const name = `${characters.join(" х ")}${kind === "video" ? ` - ${videoTitle}` : ""} [${artist}]`;
    const filename = `${name}.${extension.toLowerCase()}`;
    const modifiedAt =
      new Date(2026, 9, 7, 19, 45).getTime() - index * 5_760_000;

    return {
      id: `mock-${index + 1}`,
      filename,
      path: `C:\\Media\\Nexa demo\\${filename}`,
      kind,
      extension,
      title: kind === "video" ? name : undefined,
      videoTitle: kind === "video" ? videoTitle : undefined,
      preview: previews[scene]!,
      modifiedAt,
      createdAt: modifiedAt - 86_400_000,
      size:
        kind === "video"
          ? (128 + index * 1.35) * 1024 ** 2
          : (3 + (index % 15)) * 1024 ** 2,
      width: kind === "video" ? 3840 : 6000,
      height: kind === "video" ? 2160 : 4000,
      duration: kind === "video" ? [21, 75, 8, 156, 42][index % 5] : undefined,
      fps: kind === "video" ? 29.97 : undefined,
      codec: kind === "video" ? "H.264 / AVC" : undefined,
      bitrate: kind === "video" ? 50.2 : undefined,
      favorite: index % 7 === 1 || index % 11 === 0,
      copyrights:
        groupIndex === 2 || groupIndex === 5
          ? ["Wonderland", "Lunar Tales"]
          : [groupIndex < 3 ? "Wonderland" : "Lunar Tales"],
      artist,
      characters,
      tags: index > 11 && index % 9 === 0 ? [] : [...source.tags],
    };
  });
}
