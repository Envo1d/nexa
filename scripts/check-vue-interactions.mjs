import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { transpileModule, ModuleKind } from "typescript";
import { effectScope, nextTick, ref, shallowRef } from "vue";

// Execute the real composables with Vue reactivity and small DOM contract doubles.
// No browser, WebView, application launch or additional test dependencies are used.
const modules = new Map();

async function loadModule(relativePath) {
  const path = new URL(relativePath, import.meta.url);

  if (modules.has(path.href)) return modules.get(path.href);

  const source = await readFile(path, "utf8");

  let code = transpileModule(source, {
    compilerOptions: { module: ModuleKind.ESNext },
    fileName: fileURLToPath(path),
  }).outputText;

  code = code.replaceAll(
    'from "vue"',
    `from ${JSON.stringify(import.meta.resolve("vue"))}`,
  );

  for (const match of code.matchAll(/from "(\.\/[^"]+)"/g)) {
    const dependency = new URL(`${match[1]}.ts`, path);

    const loaded = await loadModule(dependency.href);

    code = code.replace(match[0], `from ${JSON.stringify(loaded.url)}`);
  }

  const url = `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
  const result = { url, exports: await import(url) };

  modules.set(path.href, result);

  return result;
}

const { useFocusTrap } = (
  await loadModule("../src/shared/composables/useFocusTrap.ts")
).exports;

const { useModalFocus } = (
  await loadModule("../src/shared/composables/useModalFocus.ts")
).exports;

const { useSearchSuggestions } = (
  await loadModule("../src/shared/composables/useSearchSuggestions.ts")
).exports;

function key(key, options = {}) {
  return {
    key,
    shiftKey: false,
    defaultPrevented: false,
    stopped: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    stopPropagation() {
      this.stopped = true;
    },
    ...options,
  };
}

function searchFixture() {
  const scope = effectScope();

  const search = ref("");
  const suggestions = ref([{ name: "A" }, { name: "B" }, { name: "C" }]);

  const chosen = [];
  let commits = 0;

  const state = scope.run(() =>
    useSearchSuggestions(
      search,
      suggestions,
      (name) => chosen.push(name),
      () => commits++,
      ref(null),
    ),
  );

  return {
    scope,
    search,
    suggestions,
    chosen,
    state,
    commits: () => commits,
  };
}

test("search starts ArrowUp at the last match and wraps both directions", () => {
  const { scope, state } = searchFixture();

  state.searchKeydown(key("ArrowUp"));
  assert.equal(state.suggestionIndex.value, 2);
  state.searchKeydown(key("ArrowDown"));
  assert.equal(state.suggestionIndex.value, 0);
  state.searchKeydown(key("ArrowUp"));
  assert.equal(state.suggestionIndex.value, 2);
  scope.stop();
});

test("search consumes Escape only when suggestions own it", () => {
  const { scope, state } = searchFixture();

  state.searchKeydown(key("ArrowDown"));

  const escape = key("Escape");

  state.searchKeydown(escape);
  assert.equal(escape.defaultPrevented, true);
  assert.equal(escape.stopped, true);
  assert.equal(state.searchFocused.value, false);

  const secondEscape = key("Escape");

  state.searchKeydown(secondEscape);
  assert.equal(secondEscape.stopped, false);
  scope.stop();
});

test("Enter chooses an active match or commits the typed query", () => {
  const fixture = searchFixture();

  fixture.state.searchKeydown(key("ArrowDown"));
  fixture.state.searchKeydown(key("Enter"));
  assert.deepEqual(fixture.chosen, ["A"]);
  fixture.state.searchKeydown(key("Enter"));
  assert.equal(fixture.commits(), 1);
  fixture.scope.stop();
});

test("catalog changes reset active matches and empty catalogs remain safe", async () => {
  const fixture = searchFixture();

  fixture.state.searchKeydown(key("ArrowDown"));
  fixture.suggestions.value = [];

  await nextTick();

  assert.equal(fixture.state.suggestionIndex.value, -1);
  fixture.state.searchKeydown(key("ArrowUp"));
  fixture.state.searchKeydown(key("Enter"));
  assert.equal(fixture.commits(), 1);
  fixture.scope.stop();
});

test("IME confirmation does not navigate or commit search", () => {
  const fixture = searchFixture();

  const event = key("Enter", { isComposing: true });

  fixture.state.searchKeydown(event);
  assert.equal(fixture.commits(), 0);
  assert.equal(event.defaultPrevented, false);
  fixture.scope.stop();
});

class Control {
  tabIndex = 0;
  isConnected = true;
  disabled = false;
  hidden = false;
  visible = true;
  focus() {
    document.activeElement = this;
  }
  matches() {
    return this.disabled;
  }
  closest() {
    return this.hidden ? this : null;
  }
  getClientRects() {
    return this.visible ? [{}] : [];
  }
}

globalThis.HTMLElement = Control;
globalThis.document = { activeElement: null };

function focusFixture() {
  const first = new Control();
  const last = new Control();
  const excluded = [new Control(), new Control(), new Control(), new Control()];

  excluded[0].disabled = true;
  excluded[1].hidden = true;
  excluded[2].visible = false;
  excluded[3].tabIndex = -1;

  const root = new Control();

  root.querySelectorAll = () => [
    excluded[0],
    first,
    last,
    ...excluded.slice(1),
  ];

  return {
    first,
    last,
    root,
    ...useFocusTrap(shallowRef(root)),
  };
}

test("Tab wraps only through visible enabled controls", () => {
  const { first, last, trapFocus } = focusFixture();

  last.focus();
  trapFocus(key("Tab"));
  assert.equal(document.activeElement, first);
  trapFocus(key("Tab", { shiftKey: true }));
  assert.equal(document.activeElement, last);
});

test("Tab from a programmatically focused heading stays in the modal", () => {
  const { first, last, trapFocus } = focusFixture();

  new Control().focus();
  trapFocus(key("Tab"));
  assert.equal(document.activeElement, first);
  new Control().focus();
  trapFocus(key("Tab", { shiftKey: true }));
  assert.equal(document.activeElement, last);
});

test("an empty modal consumes Tab and an owned event is left alone", () => {
  const root = new Control();

  root.querySelectorAll = () => [];

  const { trapFocus } = useFocusTrap(shallowRef(root));

  const event = key("Tab");

  trapFocus(event);
  assert.equal(event.defaultPrevented, true);
  assert.equal(document.activeElement, root);

  const outside = new Control();

  outside.focus();
  trapFocus(key("Tab", { defaultPrevented: true }));
  assert.equal(document.activeElement, outside);
});

test("disposing a modal cancels its pending initial focus", async () => {
  const scope = effectScope();

  const open = ref(false);

  const target = new Control();
  const trigger = new Control();

  trigger.focus();
  scope.run(() => useModalFocus(open, shallowRef(target), shallowRef(null)));
  open.value = true;

  await nextTick();

  scope.stop();

  await nextTick();

  assert.equal(document.activeElement, trigger);
});
