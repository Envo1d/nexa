import type { AppTheme } from "../themes";

export type InterfaceLocale = "en" | "ru";

export type AppLocale = "system" | InterfaceLocale;

export interface AppConfig {
  libraryPath: string | null;
  stagingPath: string | null;
  copyrightDirectory: boolean;
  artistDirectory: boolean;
  characterSeparator: string;
  videoSubfolder: string;
  gifSubfolder: string;
  syncStructure: boolean;
  autoScan: boolean;
  autoUpdateCheck: boolean;
  locale: AppLocale;
  theme: AppTheme;
}

export interface ConfigSnapshot {
  settings: AppConfig;
  paths: {
    configPath: string;
    databasePath: string;
    cachePath: string;
    logsPath: string;
  };
}

export function defaultConfig(): AppConfig {
  return {
    libraryPath: null,
    stagingPath: null,
    copyrightDirectory: true,
    artistDirectory: true,
    characterSeparator: " х ",
    videoSubfolder: "Videos",
    gifSubfolder: "Animations",
    syncStructure: false,
    autoScan: false,
    autoUpdateCheck: false,
    locale: "system",
    theme: "midnight-violet",
  };
}
