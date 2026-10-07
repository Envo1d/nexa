import type { MediaItem } from "../types/media";

// Self-contained illustrative previews: the review build works without network or real files.
function landscape(scene: number): string {
  const palettes = [
    ["#8878b6", "#efb8a1", "#233c52", "#122b39", "#5b8896"],
    ["#472561", "#f597ae", "#272443", "#151a30", "#624775"],
    ["#16372e", "#729068", "#2b5847", "#0d2823", "#72b8a9"],
    ["#597a99", "#f1bb8e", "#536576", "#253d52", "#b78666"],
    ["#1d607a", "#81c8ce", "#286779", "#124456", "#78bcbf"],
    ["#151d44", "#6969b6", "#292344", "#101b30", "#ae6691"],
    ["#5a777f", "#e2c6a0", "#4b626c", "#213c47", "#799095"],
    ["#5b6540", "#e9bf62", "#576242", "#243d30", "#a09850"],
    ["#3773a0", "#b9cbe1", "#416b87", "#1f4058", "#6a9aad"],
    ["#7b6985", "#f2bb8e", "#b77354", "#734635", "#bb8264"],
    ["#304369", "#c591a3", "#465574", "#202c44", "#78859f"],
    ["#102849", "#42968e", "#265977", "#102f48", "#387f8d"],
  ][scene]!;
  const [sky, light, far, near, water] = palettes;
  const city = [1, 5, 10].includes(scene);
  const forest = scene === 2 || scene === 7;
  let objects = "";
  if (city) {
    for (let i = 0; i < 36; i++) {
      const height = 22 + ((i * 47) % 110);
      const x = i * 19;
      objects += `<rect x="${x}" y="${240 - height}" width="17" height="${height + 90}" fill="${i % 2 ? near : far}"/>`;
      for (let row = 0; row < height / 12; row++) {
        objects += `<path d="M${x + 4} ${245 - height + row * 12}h3m4 0h3" stroke="${i % 3 ? "#f1c695" : "#cf89c1"}" opacity=".65" stroke-width="2"/>`;
      }
    }
  } else if (forest) {
    for (let i = 0; i < 26; i++) {
      const x = i * 27;
      const top = 48 + ((i * 23) % 95);
      objects += `<path d="M${x} ${top}l-36 134h22l-36 52h100l-36-52h22Z" fill="${i % 2 ? near : far}" opacity=".85"/><path d="M${x} ${top + 100}v180" stroke="${near}" stroke-width="7"/>`;
    }
    if (scene === 2)
      objects += `<path d="M350 135Q323 185 329 234Q314 255 285 288h99q-54-41-34-153" fill="#daece1" opacity=".9"/>`;
  } else if (scene === 9) {
    objects += `<path d="M0 237Q145 131 338 235T640 204v156H0Z" fill="${far}"/><path d="M0 283Q219 172 453 293T640 265v95H0Z" fill="${near}"/>`;
  } else if (scene === 4) {
    objects += `<path d="M640 130Q430 211 373 360H640Z" fill="#cebf9f"/><path d="M640 114Q396 201 352 360" fill="none" stroke="#deeeec" stroke-width="14" opacity=".75"/><path d="M0 266Q140 216 297 274M25 306Q149 266 283 318" fill="none" stroke="#abd3d6" stroke-width="4" opacity=".4"/>`;
  } else {
    objects += `<path d="M0 208L75 133l48 37L236 57l77 85 49-29 78 83 77-77 123 100v141H0Z" fill="${far}"/><path d="M171 132l65-75 48 53-44-14-17 29-20-10Z" fill="#e5e6ed" opacity=".8"/><path d="M0 247l106-73 93 88 159-98 109 94 173-79v181H0Z" fill="${near}"/><path d="M0 260H640V360H0Z" fill="${water}" opacity=".86"/><path d="M0 275h640M90 295h410M180 322h380" stroke="#c3d2db" opacity=".2"/>`;
    if (scene === 8)
      objects += `<path d="M278 360l30-100h26l41 100Z" fill="#263940"/><path d="M302 275h40m-45 17h51m-57 23h66m-74 26h84" stroke="#8b9698" stroke-width="2"/>`;
  }
  const aurora =
    scene === 11
      ? `<path d="M-30 95Q190 214 345 49T690 87" fill="none" stroke="#7cdfb0" stroke-width="38" opacity=".4" filter="url(#blur)"/>`
      : "";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360"><defs><linearGradient id="sky" x2="0" y2="1"><stop stop-color="${sky}"/><stop offset="1" stop-color="${light}"/></linearGradient><radialGradient id="glow"><stop stop-color="${light}" stop-opacity=".65"/><stop offset="1" stop-color="${light}" stop-opacity="0"/></radialGradient><filter id="blur"><feGaussianBlur stdDeviation="12"/></filter><filter id="grain"><feTurbulence baseFrequency=".7" numOctaves="3" stitchTiles="stitch"/><feColorMatrix type="saturate" values="0"/></filter></defs><path fill="url(#sky)" d="M0 0h640v360H0z"/><ellipse cx="440" cy="140" rx="230" ry="150" fill="url(#glow)"/><path d="M-50 90Q150 140 340 85T690 70M-50 128Q180 172 400 110T690 130" fill="none" stroke="${light}" stroke-width="13" opacity=".24" filter="url(#blur)"/>${aurora}${objects}<path fill="#fff" opacity=".045" filter="url(#grain)" d="M0 0h640v360H0z"/></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

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
  { name: "Northern_Lights", tags: ["Nature", "Night"] },
];
const previews = scenes.map((_, index) => landscape(index));

export function createMockMedia(): MediaItem[] {
  return Array.from({ length: 480 }, (_, index) => {
    const scene = index % scenes.length;
    const source = scenes[scene]!;
    const kind = [1, 4, 7, 10, 11].includes(scene) ? "video" : "image";
    const extension = kind === "video" ? "MP4" : scene === 8 ? "PNG" : "JPG";
    const filename = `${source.name}_${String(index + 1).padStart(4, "0")}.${extension.toLowerCase()}`;
    const modifiedAt =
      new Date(2026, 9, 7, 19, 45).getTime() - index * 5_760_000;
    return {
      id: `mock-${index + 1}`,
      filename,
      path: `C:\\Media\\Nexa demo\\${filename}`,
      kind,
      extension,
      title: kind === "video" ? source.title : undefined,
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
      tags: index > 11 && index % 9 === 0 ? [] : [...source.tags],
    };
  });
}
