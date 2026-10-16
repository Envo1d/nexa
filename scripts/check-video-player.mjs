import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test, { mock } from "node:test";
import { transpileModule, ModuleKind } from "typescript";
import { nextTick } from "vue";

// Run real script-setup operations against DOM/media contract doubles.
// This does not launch a browser or claim native decoding/rendering validation.
const metricsCode = transpileModule(
  await readFile(new URL("../src/features/media/playbackMetrics.ts", import.meta.url), "utf8"),
  { compilerOptions: { module: ModuleKind.ESNext } },
).outputText;

const metricsUrl = `data:text/javascript;base64,${Buffer.from(metricsCode).toString("base64")}`;

const { frameRates, seekPosition, bufferedSeconds } = await import(metricsUrl);

let fullscreenCode = transpileModule(
  await readFile(new URL("../src/features/media/usePlayerFullscreen.ts", import.meta.url), "utf8"),
  { compilerOptions: { module: ModuleKind.ESNext } },
).outputText;

fullscreenCode = fullscreenCode.replace('from "vue"', `from ${JSON.stringify(import.meta.resolve("vue"))}`);
fullscreenCode = fullscreenCode.replace('import { invoke, isTauri } from "@tauri-apps/api/core";', 'const isTauri = () => globalThis.__nexaFullscreenTest?.native ?? false; const invoke = (...args) => globalThis.__nexaFullscreenTest.invoke(...args);');

const fullscreenUrl = `data:text/javascript;base64,${Buffer.from(fullscreenCode).toString("base64")}`;

const { usePlayerFullscreen } = await import(fullscreenUrl);

test("submitted frame rate and callback cadence remain distinct at 120/60 Hz", () => {
  assert.deepEqual(frameRates({ time: 1000, frames: 30, callbacks: 20 }, { time: 2000, frames: 150, callbacks: 80 }), { submitted: 120, callbacks: 60 });
  assert.equal(frameRates({ time: 1000, frames: 30, callbacks: 20 }, { time: 1000, frames: 150, callbacks: 80 }), null);
  assert.equal(frameRates({ time: 1000, frames: 30, callbacks: 20 }, { time: 2000, frames: 1, callbacks: 21 }), null);
});

test("seeking is bounded and buffering counts only the current contiguous range", () => {
  assert.equal(seekPosition(-5, 100), 0);
  assert.equal(seekPosition(105, 100), 100);
  assert.equal(seekPosition(5, Infinity), null);
  assert.equal(seekPosition(NaN, 100), null);
  assert.equal(bufferedSeconds({ length: 2, start: (i) => [0, 20][i], end: (i) => [10, 30][i] }, 5), 5);
  assert.equal(bufferedSeconds({ length: 2, start: (i) => [0, 20][i], end: (i) => [10, 30][i] }, 15), 0);
});

let fixtureNumber = 0;

async function playerFixture() {
  globalThis.__nexaFullscreenTest = { native: false };

  const hooks = { mounted: [], unmounted: [] };

  const events = new Map();

  const documentDouble = {
    fullscreenElement: null, hidden: false,
    addEventListener: (type, handler) => events.set(type, handler),
    removeEventListener: (type) => events.delete(type),
    exitFullscreen: async () => { documentDouble.fullscreenElement = null; events.get("fullscreenchange")?.(); },
  };

  globalThis.document = documentDouble;
  globalThis.__nexaVideoPlayerTest = hooks;

  const source = await readFile(new URL("../src/components/NexaVideoPlayer.vue", import.meta.url), "utf8");

  const setup = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];

  let code = transpileModule(setup + "\nexport { player, video, diagnostics, duration, position, playing, togglePlayback, seek, toggleFullscreen, fullscreen, keyboard, stop, feedback, timeUpdate, sync, controlsVisible, toolbarVisible, setVolume, toggleMute, setSpeed, speed };", { compilerOptions: { module: ModuleKind.ESNext } }).outputText;

  code = code.replace(/import \{[\s\S]*?\} from "vue";/, `import { computed, nextTick, ref, watch } from ${JSON.stringify(import.meta.resolve("vue"))}; const onMounted = (hook) => globalThis.__nexaVideoPlayerTest.mounted.push(hook); const onBeforeUnmount = (hook) => globalThis.__nexaVideoPlayerTest.unmounted.push(hook);`);
  code = code.replace(/import \{ useI18n \} from "vue-i18n";/, 'const useI18n = () => ({ t: (key) => key });');
  code = code.replace(/import AppIcon from .*?;\n/, "");
  code = code.replace(/import AppSelect from .*?;\n/, "");
  code = code.replace(/import \{ useFocusTrap \} from .*?;/, 'const useFocusTrap = () => ({ trapFocus: () => {} });');
  code = code.replace(/from "\.\.\/features\/media\/playbackMetrics"/, `from ${JSON.stringify(metricsUrl)}`);
  code = code.replace(/from "\.\.\/features\/media\/usePlayerFullscreen"/, `from ${JSON.stringify(fullscreenUrl)}`);
  code = code.replace(/import \{ formatDuration \} from .*?;/, 'const formatDuration = (value) => String(value);');
  code = 'const defineProps = () => ({ source: "asset://test", filename: "test.mp4" }); const defineEmits = () => () => {}; const defineExpose = () => {};\n' + code + `\n// fixture ${fixtureNumber++}`;

  const module = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);

  let loads = 0;

  let cancelled = 0;

  const media = {
    paused: true, ended: false, duration: 100, currentTime: 10,
    volume: 1, muted: false, playbackRate: 1,
    buffered: { length: 0, start: () => 0, end: () => 0 },
    getAttribute: () => "asset://test",
    play: async function () { this.paused = false; },
    pause: function () { this.paused = true; },
    load: () => { loads++; }, removeAttribute: () => {},
    requestVideoFrameCallback: () => 7,
    cancelVideoFrameCallback: () => { cancelled++; },
    getVideoPlaybackQuality: () => ({ droppedVideoFrames: 2, totalVideoFrames: 120 }),
  };

  module.video.value = media;
  module.player.value = { focus: () => {}, requestFullscreen: async function () { documentDouble.fullscreenElement = this; events.get("fullscreenchange")?.(); } };

  for (const mounted of hooks.mounted) mounted();

  return { module, documentDouble, hooks, loads: () => loads, cancelled: () => cancelled };
}

test("fullscreen keeps the same playing media without source reload", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    await module.togglePlayback();
    module.video.value.currentTime = 42;
    const media = module.video.value;
    await module.toggleFullscreen();
    assert.equal(module.fullscreen.value, true);
    assert.equal(module.video.value, media);
    assert.equal(module.video.value.currentTime, 42);
    assert.equal(module.video.value.paused, false);
    assert.equal(fixture.loads(), 0);
    await module.toggleFullscreen();
    assert.equal(module.fullscreen.value, false);
    assert.equal(fixture.loads(), 0);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

test("keyboard seek is owned by the player and unmount cancels diagnostics/unloads media", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    module.duration.value = 100;
    module.position.value = 10;

    const event = { key: "ArrowRight", target: { closest: () => null }, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; } };

    module.keyboard(event);
    assert.equal(module.video.value.currentTime, 15);
    assert.equal(event.defaultPrevented, true);
    assert.equal(event.stopped, true);
    module.diagnostics.value = true;
    await nextTick();
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }

  assert.equal(fixture.cancelled(), 1);
  assert.equal(fixture.loads(), 1);
  assert.equal(module.video.value.paused, true);
});

function nativeFixture(invoke) {
  const documentDouble = {
    fullscreenElement: null,
    exitFullscreen: async () => { documentDouble.fullscreenElement = null; },
  };

  const element = { requestFullscreen: async () => { documentDouble.fullscreenElement = element; } };

  globalThis.document = documentDouble;
  globalThis.__nexaFullscreenTest = { native: true, invoke };

  return { documentDouble, element, player: { value: element } };
}

test("native fullscreen enters the DOM top layer and restores its session-owned window", async () => {
  const calls = [];

  const fixture = nativeFixture(async (command, args) => {
    assert.equal(command, "set_viewer_fullscreen");
    calls.push(args);
    return args.enabled;
  });

  let changed = 0;

  const state = usePlayerFullscreen(fixture.player, () => { changed++; }, () => assert.fail("Unexpected failure"));
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, true);
  assert.equal(fixture.documentDouble.fullscreenElement, fixture.element);
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, false);
  assert.equal(fixture.documentDouble.fullscreenElement, null);
  assert.equal(changed, 2);
  assert.deepEqual(calls.map((call) => call.enabled), [true, false]);
  assert.equal(calls[0].session, calls[1].session);
  assert.ok(calls[0].session.length);
  await state.disposeFullscreen();
});

test("late native entry after disposal restores its own window lease", async () => {
  const calls = [];

  let resolve;

  const fixture = nativeFixture(async (_command, args) => {
    calls.push(args);
    if (args.enabled) await new Promise((done) => { resolve = done; });
    return args.enabled;
  });

  fixture.element.requestFullscreen = () => assert.fail("Disposed player must not enter DOM fullscreen");

  let changed = 0;

  const state = usePlayerFullscreen(fixture.player, () => { changed++; }, () => assert.fail("Unexpected failure"));

  const pending = state.toggleFullscreen();
  await state.toggleFullscreen();
  assert.equal(calls.length, 1);
  await state.disposeFullscreen();
  resolve();
  await pending;
  assert.deepEqual(calls.map((call) => call.enabled), [true, false]);
  assert.equal(calls[0].session, calls[1].session);
  assert.equal(state.fullscreen.value, false);
  assert.equal(state.fullscreenBusy.value, false);
  assert.equal(changed, 0);
});

test("native failures preserve the previous UI mode and allow restoration retry", async () => {
  let reject = true;

  let failures = 0;

  const fixture = nativeFixture(async () => {
    if (reject) throw new Error("Window operation failed");
  });

  const state = usePlayerFullscreen(fixture.player, () => {}, () => { failures++; });
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, false);
  assert.equal(state.fullscreenBusy.value, false);
  reject = false;
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, true);
  reject = true;
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, true);
  reject = false;
  await state.disposeFullscreen();
  assert.equal(state.fullscreen.value, false);
  assert.equal(failures, 2);
});

test("browser-controlled DOM exit restores native fullscreen without a second toggle", async () => {
  const calls = [];

  const fixture = nativeFixture(async (_command, args) => { calls.push(args.enabled); });

  const state = usePlayerFullscreen(fixture.player, () => {}, () => assert.fail("Unexpected failure"));
  await state.toggleFullscreen();
  fixture.documentDouble.fullscreenElement = null;
  await state.fullscreenChanged();
  assert.equal(state.fullscreen.value, false);
  assert.deepEqual(calls, [true, false]);
});

test("DOM entry rejection rolls back the native window and allows another attempt", async () => {
  const calls = [];

  let failures = 0;

  const fixture = nativeFixture(async (_command, args) => { calls.push(args.enabled); });

  const request = fixture.element.requestFullscreen;
  fixture.element.requestFullscreen = async () => { throw new Error("Activation expired"); };

  const state = usePlayerFullscreen(fixture.player, () => {}, () => { failures++; });
  await state.toggleFullscreen();
  assert.deepEqual(calls, [true, false]);
  assert.equal(state.fullscreen.value, false);
  assert.equal(state.fullscreenBusy.value, false);
  assert.equal(failures, 1);
  fixture.element.requestFullscreen = request;
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, true);
  await state.disposeFullscreen();
});

test("unmount during DOM entry cleans the captured element after its ref is cleared", async () => {
  const calls = [];

  const fixture = nativeFixture(async (_command, args) => { calls.push(args.enabled); });

  let entered;

  let complete;

  const requested = new Promise((resolve) => { entered = resolve; });

  fixture.element.requestFullscreen = async () => {
    entered();
    await new Promise((resolve) => { complete = resolve; });
    fixture.documentDouble.fullscreenElement = fixture.element;
  };

  const state = usePlayerFullscreen(fixture.player, () => {}, () => assert.fail("Unexpected failure"));

  const pending = state.toggleFullscreen();
  await requested;
  fixture.player.value = null;
  await state.disposeFullscreen();
  complete();
  await pending;
  assert.equal(fixture.documentDouble.fullscreenElement, null);
  assert.equal(state.fullscreen.value, false);
  assert.deepEqual(calls, [true, false]);
});

test("external DOM exit during entry releases native state without entering it again", async () => {
  const calls = [];

  const fixture = nativeFixture(async (_command, args) => { calls.push(args.enabled); });

  fixture.element.requestFullscreen = async () => {
    fixture.documentDouble.fullscreenElement = fixture.element;
    fixture.documentDouble.fullscreenElement = null;
  };

  const state = usePlayerFullscreen(fixture.player, () => {}, () => assert.fail("Unexpected failure"));
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, false);
  assert.deepEqual(calls, [true, false]);
});

test("DOM exit failure keeps ownership and retry does not exit another fullscreen element", async () => {
  const calls = [];

  let failures = 0;

  const fixture = nativeFixture(async (_command, args) => { calls.push(args.enabled); });

  const state = usePlayerFullscreen(fixture.player, () => {}, () => { failures++; });
  await state.toggleFullscreen();
  fixture.documentDouble.exitFullscreen = async () => { throw new Error("Exit failed"); };
  await state.toggleFullscreen();
  assert.equal(state.fullscreen.value, true);
  assert.equal(failures, 1);
  assert.deepEqual(calls, [true]);

  const other = {};
  fixture.documentDouble.fullscreenElement = other;
  await state.fullscreenChanged();
  assert.equal(fixture.documentDouble.fullscreenElement, other);
  assert.equal(state.fullscreen.value, false);
  assert.deepEqual(calls, [true, false]);
});

test("playback position publication is bounded while seek and pause updates stay immediate", async () => {
  globalThis.__nexaFullscreenTest = { native: false };

  const fixture = await playerFixture();

  const { module } = fixture;

  let now = 1000;

  const clock = mock.method(performance, "now", () => now);

  try {
    module.video.value.currentTime = 1;
    module.timeUpdate();
    assert.equal(module.position.value, 1);

    for (now = 1010; now < 1250; now += 10) {
      module.video.value.currentTime = 2;
      module.timeUpdate();
      assert.equal(module.position.value, 1);
    }

    module.timeUpdate();
    assert.equal(module.position.value, 2);
    now = 1260;
    module.duration.value = 100;
    module.seek(75);
    assert.equal(module.position.value, 75);
    now = 1270;
    module.video.value.currentTime = 75.1;
    module.video.value.paused = true;
    module.sync();
    assert.equal(module.position.value, 75.1);
    assert.equal(module.playing.value, false);
    now = 1280;
    module.video.value.currentTime = 76;
    module.timeUpdate();
    assert.equal(module.position.value, 75.1);
  } finally {
    clock.mock.restore();
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

test("shared speed selection updates media rate and preserves listbox arrow ownership", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    module.setSpeed("1.5");
    assert.equal(module.video.value.playbackRate, 1.5);
    assert.equal(module.speed.value, 1.5);
    module.setSpeed("unsupported");
    module.setSpeed("-1");
    assert.equal(module.video.value.playbackRate, 1.5);
    const event = keyEvent("ArrowRight", {
      target: { closest: (selector) => selector.includes("[role='listbox']") ? {} : null },
    });
    module.keyboard(event);
    assert.equal(event.defaultPrevented, false);
    assert.equal(module.video.value.currentTime, 10);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

function keyEvent(key, { control = false, editable = false, ...options } = {}) {
  return {
    key,
    target: { closest: (selector) => selector.includes("input:not") ? (editable ? {} : null) : (control ? {} : null) },
    defaultPrevented: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
    ...options,
  };
}

test("Space and physical F/M shortcuts override focused controls without repeat toggles", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    const space = keyEvent(" ", { control: true });
    module.keyboard(space);
    assert.equal(module.video.value.paused, false);
    assert.equal(space.defaultPrevented, true);
    module.keyboard(keyEvent(" ", { control: true, repeat: true }));
    assert.equal(module.video.value.paused, false);
    module.keyboard(keyEvent(" ", { control: true }));
    assert.equal(module.video.value.paused, true);

    module.keyboard(keyEvent("ь", { code: "KeyM", control: true }));
    assert.equal(module.video.value.muted, true);
    module.keyboard(keyEvent("ь", { code: "KeyM", control: true, repeat: true }));
    assert.equal(module.video.value.muted, true);
    module.keyboard(keyEvent("m", { control: true }));
    assert.equal(module.video.value.muted, false);

    module.keyboard(keyEvent("а", { code: "KeyF", control: true }));
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(module.fullscreen.value, true);
    assert.equal(module.controlsVisible.value, false);
    module.keyboard(keyEvent("f", { control: true }));
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(module.fullscreen.value, false);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

test("T toggles diagnostics in both modes, preserves editing and cancels metrics on disable", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    const event = keyEvent("t", { control: true });
    module.keyboard(event);
    await nextTick();
    assert.equal(module.diagnostics.value, true);
    assert.equal(event.defaultPrevented, true);
    module.keyboard(keyEvent("t", { repeat: true }));
    assert.equal(module.diagnostics.value, true);
    module.keyboard(keyEvent("t"));
    await nextTick();
    assert.equal(module.diagnostics.value, false);
    assert.equal(fixture.cancelled(), 1);
    await module.toggleFullscreen();
    module.keyboard(keyEvent("е", { code: "KeyT" }));
    await nextTick();
    assert.equal(module.diagnostics.value, true);
    for (const ignored of [keyEvent("t", { editable: true }), keyEvent("t", { ctrlKey: true })]) {
      module.keyboard(ignored);
      assert.equal(ignored.defaultPrevented, false);
      assert.equal(module.diagnostics.value, true);
    }
    module.keyboard(keyEvent("е", { code: "KeyT" }));
    await nextTick();
    assert.equal(module.diagnostics.value, false);
    assert.equal(fixture.cancelled(), 2);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

test("fullscreen arrows seek actual media time, bound volume and reveal temporary controls", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    module.duration.value = 100;
    await module.toggleFullscreen();
    module.video.value.currentTime = 42;
    module.position.value = 10;
    module.keyboard(keyEvent("ArrowRight"));
    assert.equal(module.video.value.currentTime, 47);
    module.keyboard(keyEvent("ArrowLeft", { repeat: true }));
    assert.equal(module.video.value.currentTime, 42);
    module.video.value.currentTime = 99;
    module.keyboard(keyEvent("ArrowRight"));
    assert.equal(module.video.value.currentTime, 100);
    module.video.value.currentTime = 1;
    module.keyboard(keyEvent("ArrowLeft"));
    assert.equal(module.video.value.currentTime, 0);

    module.keyboard(keyEvent("ArrowDown"));
    assert.equal(module.video.value.volume, 0.95);
    module.keyboard(keyEvent("ArrowUp"));
    module.keyboard(keyEvent("ArrowUp"));
    assert.equal(module.video.value.volume, 1);
    module.setVolume(0.01);
    module.keyboard(keyEvent("ArrowDown"));
    assert.equal(module.video.value.volume, 0);
    assert.equal(module.video.value.muted, true);
    module.keyboard(keyEvent("ArrowUp", { repeat: true }));
    assert.equal(module.video.value.volume, 0.05);
    assert.equal(module.video.value.muted, false);
    assert.equal(module.controlsVisible.value, true);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

test("fullscreen controls hide after two seconds and timers reset on activity, exit and disposal", async () => {
  const pending = new Map();

  let id = 0;

  const timeout = mock.method(globalThis, "setTimeout", (callback, delay) => {
    assert.equal(delay, 2000);
    pending.set(++id, callback);
    return id;
  });

  const clear = mock.method(globalThis, "clearTimeout", (timer) => pending.delete(timer));

  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    module.duration.value = 100;
    await module.toggleFullscreen();
    module.keyboard(keyEvent(" "));
    assert.equal(module.controlsVisible.value, false);
    assert.equal(pending.size, 0);
    module.keyboard(keyEvent("ArrowRight"));
    const first = id;
    module.keyboard(keyEvent("ArrowUp"));
    assert.equal(pending.has(first), false);
    assert.equal(pending.size, 1);
    assert.equal(module.controlsVisible.value, true);
    pending.get(id)();
    assert.equal(module.controlsVisible.value, false);
    assert.equal(pending.size, 0);
    module.toggleMute();
    assert.equal(module.controlsVisible.value, true);
    await module.toggleFullscreen();
    assert.equal(module.controlsVisible.value, false);
    assert.equal(pending.size, 0);
    await module.toggleFullscreen();
    module.keyboard(keyEvent("ArrowLeft"));
    assert.equal(pending.size, 1);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
    timeout.mock.restore();
    clear.mock.restore();
  }

  assert.equal(module.controlsVisible.value, false);
  assert.equal(pending.size, 0);
});

test("fullscreen pause pins the toolbar through feedback expiry and resume clears stale activity", async () => {
  const pending = new Map();

  let id = 0;

  const timeout = mock.method(globalThis, "setTimeout", (callback) => {
    pending.set(++id, callback);
    return id;
  });

  const clear = mock.method(globalThis, "clearTimeout", (timer) => pending.delete(timer));

  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    module.duration.value = 100;
    await module.toggleFullscreen();
    assert.equal(module.toolbarVisible.value, true);
    module.video.value.paused = false;
    module.sync();
    assert.equal(module.toolbarVisible.value, false);
    module.seek(20);
    assert.equal(pending.size, 1);
    module.video.value.paused = true;
    module.sync();
    assert.equal(pending.size, 0);
    assert.equal(module.toolbarVisible.value, true);
    module.setVolume(0.5);
    pending.get(id)();
    assert.equal(pending.size, 0);
    assert.equal(module.controlsVisible.value, false);
    assert.equal(module.toolbarVisible.value, true);
    module.seek(25);
    module.video.value.paused = false;
    module.sync();
    assert.equal(pending.size, 0);
    assert.equal(module.toolbarVisible.value, false);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
    timeout.mock.restore();
    clear.mock.restore();
  }
});

test("windowed range arrows, text editing and modified shortcuts retain their native behavior", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    for (const event of [
      keyEvent("ArrowRight", { control: true }),
      keyEvent("ArrowUp"),
      keyEvent(" ", { editable: true }),
      keyEvent("f", { editable: true }),
      keyEvent("m", { ctrlKey: true }),
      keyEvent(" ", { isComposing: true }),
    ]) {
      module.keyboard(event);
      assert.equal(event.defaultPrevented, false);
    }

    assert.equal(module.video.value.currentTime, 10);
    assert.equal(module.video.value.paused, true);
    assert.equal(module.video.value.volume, 1);
    assert.equal(module.video.value.muted, false);
    assert.equal(module.fullscreen.value, false);
    assert.equal(module.controlsVisible.value, false);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});

test("speed listbox owns Space, arrows, Escape and typeahead without changing playback", async () => {
  const fixture = await playerFixture();

  const { module } = fixture;

  try {
    for (const key of [" ", "ArrowRight", "ArrowDown", "Escape", "f", "m", "t", "k"]) {
      const event = keyEvent(key);

      event.target = { closest: (selector) => selector.includes("[role='listbox']") ? {} : null };
      module.keyboard(event);
      assert.equal(event.defaultPrevented, false, `${key} belongs to the listbox`);
    }

    assert.equal(module.video.value.paused, true);
    assert.equal(module.video.value.currentTime, 10);
    assert.equal(module.video.value.muted, false);
    assert.equal(module.diagnostics.value, false);
    assert.equal(module.fullscreen.value, false);
  } finally {
    for (const unmount of fixture.hooks.unmounted) unmount();
  }
});
