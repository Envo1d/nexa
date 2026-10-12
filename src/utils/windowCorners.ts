export function usesSystemWindowCorners(
  platform: string,
  version: string,
): boolean {
  if (platform !== "windows") return false;

  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);

  if (!match) return false;

  const major = Number(match[1]);

  const minor = Number(match[2]);

  const build = Number(match[3]);

  // Windows 11 reports NT 10.0, starting with build 22000.
  return major === 10 && minor === 0 && build >= 22000;
}
