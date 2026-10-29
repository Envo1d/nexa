import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { transpileModule, ModuleKind } from "typescript";

// Execute the shared service against controlled IPC/event responses, without a browser.
let fixtureNumber = 0;

function deferred() {
  let resolve;
  let reject;

  const promise = new Promise((done, fail) => {
    resolve = done;
    reject = fail;
  });

  return {
    promise,
    resolve,
    reject,
  };
}

function snapshot(scanned, running = true) {
  return {
    running,
    scanned,
    extracted: scanned,
    errors: 0,
    message: "Scanning",
  };
}

async function fixture(invoke) {
  const listeners = new Set();
  let subscriptions = 0;

  globalThis.__nexaScanTest = {
    invoke,
    listen: async (name, handler) => {
      assert.equal(name, "stage-scan-status");
      subscriptions++;
      listeners.add(handler);

      return () => listeners.delete(handler);
    },
  };

  let code = transpileModule(
    await readFile(
      new URL("../src/composables/useStageScan.ts", import.meta.url),
      "utf8",
    ),
    { compilerOptions: { module: ModuleKind.ESNext } },
  ).outputText;

  code = code.replace(
    'from "vue"',
    `from ${JSON.stringify(import.meta.resolve("vue"))}`,
  );
  code = code.replace(
    'import { invoke, isTauri } from "@tauri-apps/api/core";',
    "const isTauri = () => true; const invoke = (...args) => globalThis.__nexaScanTest.invoke(...args);",
  );
  code = code.replace(
    'import { listen } from "@tauri-apps/api/event";',
    "const listen = (...args) => globalThis.__nexaScanTest.listen(...args);",
  );
  code += `\n// fixture ${fixtureNumber++}`;

  const { useStageScan } = await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  );

  return {
    useStageScan,
    listeners,
    subscriptions: () => subscriptions,
    emit: (payload) => {
      for (const listener of listeners) listener({ payload });
    },
  };
}

test("concurrent consumers share one scan subscription and initial snapshot", async () => {
  const pending = deferred();

  let reads = 0;

  const service = await fixture(async (command) => {
    assert.equal(command, "get_stage_scan_status");
    reads++;
    return pending.promise;
  });

  const first = service.useStageScan();

  const second = service.useStageScan();

  const connections = [first.connect(), second.connect()];

  pending.resolve(snapshot(10));

  await Promise.all(connections);

  assert.equal(service.subscriptions(), 1);
  assert.equal(reads, 1);
  assert.equal(first.status, second.status);
  assert.equal(second.status.value.scanned, 10);
});

test("failed initialization removes its listener and retry installs one working subscription", async () => {
  let fail = true;

  const service = await fixture(async () => {
    if (fail) throw new Error("Snapshot unavailable");

    return snapshot(1);
  });

  const scan = service.useStageScan();

  await assert.rejects(scan.connect(), /Snapshot unavailable/);

  assert.equal(service.listeners.size, 0);
  fail = false;

  await scan.connect();

  assert.equal(service.subscriptions(), 2);
  assert.equal(service.listeners.size, 1);
  service.emit(snapshot(25));
  assert.equal(scan.status.value.scanned, 25);
});

test("a live event wins over a pending initial snapshot", async () => {
  const pending = deferred();

  const requested = deferred();

  const service = await fixture(() => {
    requested.resolve();
    return pending.promise;
  });

  const scan = service.useStageScan();

  const connecting = scan.connect();

  await requested.promise;

  service.emit(snapshot(40));
  pending.resolve(snapshot(2));

  await connecting;

  assert.equal(scan.status.value.scanned, 40);
});

test("a live completion wins over the snapshot read after starting either scan type", async () => {
  for (const library of [false, true]) {
    const pending = deferred();

    const requested = deferred();

    let reads = 0;

    const service = await fixture(async (command) => {
      if (command === "get_stage_scan_status") {
        if (++reads === 1) return snapshot(0, false);

        requested.resolve();
        return pending.promise;
      }

      assert.equal(
        command,
        library ? "start_library_scan" : "start_stage_scan",
      );
    });

    const scan = service.useStageScan();

    const starting = scan.start(library);

    await requested.promise;

    service.emit(snapshot(100, false));
    pending.resolve(snapshot(3));

    await starting;

    assert.equal(scan.status.value.scanned, 100);
    assert.equal(scan.status.value.running, false);
  }
});

test("an older post-start snapshot cannot replace a newer read", async () => {
  const pending = [deferred(), deferred()];
  const requested = [deferred(), deferred()];
  let reads = 0;

  const service = await fixture(async (command) => {
    if (command !== "get_stage_scan_status") return;

    if (++reads === 1) return snapshot(0, false);

    const index = reads - 2;

    requested[index].resolve();

    return pending[index].promise;
  });

  const scan = service.useStageScan();

  await scan.connect();

  const first = scan.start();

  await requested[0].promise;

  const second = scan.start(true);

  await requested[1].promise;

  pending[1].resolve(snapshot(80));

  await second;

  pending[0].resolve(snapshot(5));

  await first;

  assert.equal(scan.status.value.scanned, 80);
});
