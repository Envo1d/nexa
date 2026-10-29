import { onScopeDispose, ref, shallowRef } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../../runtime";

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
  const deletionRequest = shallowRef<{
    items: MediaItem[];
    trigger: HTMLElement | null;
  } | null>(null);

  let resolveDeletion: ((accepted: boolean) => void) | null = null;

  function answerDeletion(accepted: boolean) {
    const resolve = resolveDeletion;

    resolveDeletion = null;
    deletionRequest.value = null;
    resolve?.(accepted);
  }

  onScopeDispose(() => answerDeletion(false));

  async function stageFileAction(
    action: string,
    items: MediaItem[],
    main = isMain(),
    verifyRevision = false,
  ) {
    if (fileActionBusy.value || !items.length) return;

    if (!isNativeData()) {
      closeContext();
      notify(`${action} is available in the production desktop application.`);

      return;
    }

    items = [...items];

    const trigger =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

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
        const accepted = await new Promise<boolean>((resolve) => {
          resolveDeletion = resolve;
          deletionRequest.value = { items, trigger };
        });

        if (!accepted) return;
      }

      const report = await invoke<{ succeeded: string[]; failed: string[] }>(
        "stage_file_action",
        {
          ids: items.map((item) => item.nativeId),
          action: operation,
          library: main,
          expectedRevisions: verifyRevision
            ? Object.fromEntries(
                items.map((item) => [item.nativeId, item.nativeRevision]),
              )
            : null,
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

  return {
    fileActionBusy,
    deletionRequest,
    answerDeletion,
    stageFileAction,
  };
}
