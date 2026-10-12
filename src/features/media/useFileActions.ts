import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";

import type { MediaItem } from "../../types/media";
import type { LibraryModel, StageModel, Notify } from "./model";

export function useFileActions(
  library: LibraryModel,
  stage: StageModel,
  isMain: () => boolean,
  closeContext: () => void,
  notify: Notify,
) {
  const fileActionBusy = ref(false);

  async function stageFileAction(
    action: string,
    items: MediaItem[],
    main = isMain(),
  ) {
    if (fileActionBusy.value) return;

    const operation =
      action === "Share"
        ? "share"
        : action === "Delete file"
          ? "delete"
          : action === "Open file" || action === "Open"
            ? "open"
            : "reveal";

    fileActionBusy.value = true;
    closeContext();

    try {
      if (operation === "delete") {
        const names = items
          .slice(0, 5)
          .map((item) => item.filename)
          .join("\n");

        const accepted = await confirm(
          `Delete ${items.length} file(s) from disk?\n\n${names}${items.length > 5 ? "\n…" : ""}\n\nFiles will be moved to the Recycle Bin when available. Windows will ask before permanently deleting files that cannot be recycled.`,
          {
            title: "Delete files",
            kind: "warning",
            okLabel: "Delete files",
            cancelLabel: "Cancel",
          },
        );

        if (!accepted) return;
      }

      const report = await invoke<{ succeeded: string[]; failed: string[] }>(
        "stage_file_action",
        {
          ids: items.map((item) => item.nativeId),
          action: operation,
          library: main,
        },
      );

      if (operation === "delete") {
        if (main) library.clearSelection();
        else stage.clearSelection();

        await Promise.all([stage.refreshRedistribution(), library.refresh()]);
      }

      if (report.failed.length)
        notify(
          `${report.succeeded.length} completed; ${report.failed.length} failed. ${report.failed[0]}`,
        );
      else if (operation === "delete")
        notify(`${report.succeeded.length} file(s) deleted.`);
    } catch (cause) {
      notify(`File action failed: ${String(cause)}`);
    } finally {
      fileActionBusy.value = false;
    }
  }

  return { fileActionBusy, stageFileAction };
}
