import { onScopeDispose, ref } from "vue";

export function useToast() {
  const toast = ref("");

  let disposed = false;

  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function notify(message: string) {
    if (disposed) return;

    toast.value = message;
    clearTimeout(toastTimer);

    toastTimer = setTimeout(() => {
      toast.value = "";
    }, 4500);
  }

  onScopeDispose(() => {
    disposed = true;
    clearTimeout(toastTimer);
  });

  return { toast, notify };
}
