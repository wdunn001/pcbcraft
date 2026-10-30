// Run with: node apps/pcbcraft-web/tests/webgpu-check.test.js
const assert = require("node:assert");
const {
  pcbcraftNeedsWebGpu,
  pcbcraftHasWebGl2,
} = require("../webgpu-check.js");

function fakeDocument(webgl2) {
  const removed = [];
  const loaders = ["script", "link"].map((name) => ({
    name,
    remove: () => removed.push(name),
  }));
  const notice = { hidden: true };
  const canvas = { hidden: false };
  const probe = {
    getContext: (kind) =>
      webgl2 && (kind === "webgl2" || kind === "experimental-webgl2")
        ? {}
        : null,
  };
  return {
    removed,
    notice,
    canvas,
    getElementById: (id) =>
      id === "needs-webgpu" ? notice : id === "pcbcraft" ? canvas : null,
    createElement: (tag) => (tag === "canvas" ? probe : {}),
    querySelectorAll: () => loaders,
  };
}

function the_message_shows_without_any_graphics_api() {
  const doc = fakeDocument(false);
  assert.strictEqual(pcbcraftNeedsWebGpu({}, doc), true);
  assert.strictEqual(doc.notice.hidden, false, "the message is shown");
  assert.strictEqual(doc.canvas.hidden, true, "the canvas is hidden");
  assert.strictEqual(doc.removed.length, 2, "the wasm loaders are removed");
}

function the_page_starts_normally_with_webgpu() {
  const doc = fakeDocument(false);
  assert.strictEqual(pcbcraftNeedsWebGpu({ gpu: {} }, doc), false);
  assert.strictEqual(doc.notice.hidden, true);
  assert.strictEqual(doc.removed.length, 0);
}

function the_page_starts_normally_with_webgl2_only() {
  const doc = fakeDocument(true);
  assert.strictEqual(pcbcraftHasWebGl2(doc), true);
  assert.strictEqual(pcbcraftNeedsWebGpu({}, doc), false);
  assert.strictEqual(doc.notice.hidden, true);
  assert.strictEqual(doc.removed.length, 0);
}

the_message_shows_without_any_graphics_api();
the_page_starts_normally_with_webgpu();
the_page_starts_normally_with_webgl2_only();
console.log("ok");
