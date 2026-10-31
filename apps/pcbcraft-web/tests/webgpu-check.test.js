// Run with: node apps/pcbcraft-web/tests/webgpu-check.test.js
const assert = require("node:assert");
const {
  pcbcraftNeedsGraphics,
  pcbcraftIsCrash,
  pcbcraftWatchForCrash,
} = require("../webgpu-check.js");

function fakeDocument(webgl2) {
  const notice = { hidden: true };
  const crashed = { hidden: true };
  const canvas = { hidden: false };
  const button = { listeners: {}, addEventListener(name, fn) { this.listeners[name] = fn; } };
  return {
    notice,
    crashed,
    canvas,
    button,
    createElement: () => ({ getContext: (kind) => (webgl2 && kind === "webgl2" ? {} : null) }),
    getElementById: (id) =>
      ({ "needs-webgpu": notice, "web-crashed": crashed, pcbcraft: canvas, "web-reload": button })[id] || null,
  };
}

function the_message_shows_with_neither_webgpu_nor_webgl2() {
  const doc = fakeDocument(false);
  assert.strictEqual(pcbcraftNeedsGraphics({}, doc), true);
  assert.strictEqual(doc.notice.hidden, false, "the message is shown");
  assert.strictEqual(doc.canvas.hidden, true, "the canvas is hidden");
}

function the_page_starts_with_webgpu_only() {
  const doc = fakeDocument(false);
  assert.strictEqual(pcbcraftNeedsGraphics({ gpu: {} }, doc), false);
  assert.strictEqual(doc.notice.hidden, true);
}

function the_page_starts_with_webgl2_only() {
  const doc = fakeDocument(true);
  assert.strictEqual(pcbcraftNeedsGraphics({}, doc), false);
  assert.strictEqual(doc.notice.hidden, true, "no notice for a WebGL 2 browser");
  assert.strictEqual(doc.canvas.hidden, false);
}

function a_wasm_trap_shows_the_crash_notice_and_reload_works() {
  const doc = fakeDocument(true);
  const listeners = {};
  let reloaded = false;
  const win = {
    addEventListener: (name, fn) => (listeners[name] = fn),
    location: { reload: () => (reloaded = true) },
  };
  pcbcraftWatchForCrash(win, doc);
  listeners.error({ error: { name: "Error", message: "ordinary script error" }, message: "x" });
  assert.strictEqual(doc.crashed.hidden, true, "an ordinary error is not a crash");
  listeners.error({ error: { name: "RuntimeError", message: "unreachable" }, message: "x" });
  assert.strictEqual(doc.crashed.hidden, false, "the crash notice is shown");
  assert.strictEqual(doc.canvas.hidden, true);
  doc.button.listeners.click();
  assert.strictEqual(reloaded, true, "the button reloads the page");
  assert.strictEqual(pcbcraftIsCrash(null, "panicked at sheet.rs"), true);
}

the_message_shows_with_neither_webgpu_nor_webgl2();
the_page_starts_with_webgpu_only();
the_page_starts_with_webgl2_only();
a_wasm_trap_shows_the_crash_notice_and_reload_works();
console.log("ok");
