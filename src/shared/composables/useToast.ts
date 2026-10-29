import { onScopeDispose, ref } from "vue";

export interface ToastOptions {
  id?: string;
  description?: string;
  kind?: "info" | "success" | "error" | "update";
  duration?: number;
  action?: { label: string; run: () => void };
}

export interface ToastMessage extends ToastOptions {
  id: string;
  message: string;
}

export type Notify = (message: string, options?: ToastOptions) => void;

export function useToast() {
  const toasts = ref<ToastMessage[]>([]);

  const timers = new Map<
    string,
    {
      remaining: number;
      started: number;
      timer?: ReturnType<typeof setTimeout>;
    }
  >();

  let sequence = 0;
  let paused = false;
  let disposed = false;

  function dismissToast(id: string) {
    clearTimeout(timers.get(id)?.timer);
    timers.delete(id);
    toasts.value = toasts.value.filter((toast) => toast.id !== id);
  }

  function schedule(id: string) {
    const entry = timers.get(id);

    if (!entry || paused) return;

    entry.started = Date.now();
    entry.timer = setTimeout(() => dismissToast(id), entry.remaining);
  }

  function notify(message: string, options: ToastOptions = {}) {
    if (disposed) return;

    const id = options.id ?? `toast-${++sequence}`;

    dismissToast(id);
    toasts.value.push({
      ...options,
      id,
      message,
    });

    if (toasts.value.length > 5) dismissToast(toasts.value[0]!.id);

    const duration = options.duration ?? (options.action ? 10000 : 4500);

    if (duration > 0) {
      timers.set(id, { remaining: duration, started: Date.now() });
      schedule(id);
    }
  }

  function pauseToasts(value: boolean) {
    if (paused === value || disposed) return;

    paused = value;

    for (const [id, entry] of timers) {
      if (paused) {
        clearTimeout(entry.timer);
        entry.remaining = Math.max(
          0,
          entry.remaining - (Date.now() - entry.started),
        );
      } else schedule(id);
    }
  }

  function runToastAction(toast: ToastMessage) {
    if (disposed || !toasts.value.some((entry) => entry.id === toast.id))
      return;

    dismissToast(toast.id);
    toast.action?.run();
  }

  onScopeDispose(() => {
    disposed = true;

    for (const entry of timers.values()) clearTimeout(entry.timer);

    timers.clear();
  });

  return {
    toasts,
    notify,
    dismissToast,
    pauseToasts,
    runToastAction,
  };
}
