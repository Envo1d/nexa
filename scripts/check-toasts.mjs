import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { effectScope, onScopeDispose, ref } from "vue";
import { transpileModule, ModuleKind } from "typescript";

const source = await readFile(
  new URL("../src/shared/composables/useToast.ts", import.meta.url),
  "utf8",
);

const code = transpileModule(source, {
  compilerOptions: { module: ModuleKind.CommonJS },
}).outputText;

function fixture() {
  let now = 0;
  let sequence = 0;
  const pending = new Map();
  const exports = {};

  const scope = effectScope();

  vm.runInNewContext(code, {
    exports,
    require: () => ({ ref, onScopeDispose }),
    Date: { now: () => now },
    setTimeout: (run, delay) => {
      const id = ++sequence;

      pending.set(id, { run, at: now + delay });
      return id;
    },
    clearTimeout: (id) => pending.delete(id),
  });

  const toast = scope.run(exports.useToast);

  return {
    ...toast,
    pending,
    stop: () => scope.stop(),
    advance: (elapsed) => {
      now += elapsed;

      for (const [id, entry] of pending) {
        if (entry.at <= now) {
          pending.delete(id);
          entry.run();
        }
      }
    },
  };
}

test("notifications coexist, expire independently and stay bounded", () => {
  const state = fixture();

  state.notify("First");
  state.advance(1000);
  state.notify("Second");
  state.advance(3500);
  assert.equal(state.toasts.value.length, 1);
  assert.equal(state.toasts.value[0].message, "Second");

  for (let index = 0; index < 8; index++) state.notify(`Message ${index}`);

  assert.equal(state.toasts.value.length, 5);
  assert.equal(state.toasts.value[0].message, "Message 3");
  assert.equal(state.pending.size, 5);
  state.stop();
});

test("interaction pauses remaining time and newly arriving notifications", () => {
  const state = fixture();

  state.notify("First");
  state.advance(2000);
  state.pauseToasts(true);
  state.notify("Second");
  state.advance(20000);
  assert.equal(state.toasts.value.length, 2);
  assert.equal(state.pending.size, 0);
  state.pauseToasts(false);
  state.advance(2499);
  assert.equal(state.toasts.value.length, 2);
  state.advance(1);
  assert.equal(state.toasts.value.length, 1);
  state.advance(2000);
  assert.equal(state.toasts.value.length, 0);
  state.stop();
});

test("persistent actions survive expiration and run only when activated", () => {
  const state = fixture();

  let actions = 0;

  state.notify("Update", {
    id: "update",
    duration: 0,
    action: { label: "View", run: () => actions++ },
  });
  state.advance(60000);
  assert.equal(actions, 0);
  assert.equal(state.toasts.value.length, 1);
  const notification = state.toasts.value[0];

  state.runToastAction(notification);
  state.runToastAction(notification);
  assert.equal(actions, 1);
  assert.equal(state.toasts.value.length, 0);
  state.stop();
});

test("replacement, dismissal and disposal release timers and reject late notices", () => {
  const state = fixture();

  state.notify("Old", { id: "same" });
  state.advance(3000);
  state.notify("New", { id: "same" });
  assert.equal(state.pending.size, 1);
  state.advance(1500);
  assert.equal(state.toasts.value[0].message, "New");
  state.dismissToast("same");
  assert.equal(state.pending.size, 0);
  state.notify("Unmount");
  state.stop();
  assert.equal(state.pending.size, 0);
  state.notify("Late");
  assert.equal(state.toasts.value.length, 1);
});
