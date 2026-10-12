import type { useLibrary } from "../../composables/useLibrary";
import type { useStage } from "../../composables/useStage";

export type LibraryModel = ReturnType<typeof useLibrary>;

export type StageModel = ReturnType<typeof useStage>;

export type Notify = (message: string) => void;
