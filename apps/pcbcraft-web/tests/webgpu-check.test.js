// Run with: node apps/pcbcraft-web/tests/webgpu-check.test.js
const assert = require("node:assert");
const { pcbcraftNeedsWebGpu } = require("../webgpu-check.js");

function fakeDocument() {
  const removed = [];
  const loaders = ["script", "link"].map((name) => ({ name, remove: () => removed.push(name) }));
  const notice = { hidden: true };
  const canvas = { hidden: false };
  return {
    removed,
    notice,
    canvas,
    getElementById: (id) => (id === "needs-webgpu" ? notice : id === "pcbcraft" ? canvas : null),
    querySelectorAll: () => loaders,
  };
}

function the_message_shows_without_webgpu() {
  const doc = fakeDocument();
  assert.strictEqual(pcbcraftNeedsWebGpu({}, doc), true);
  assert.strictEqual(doc.notice.hidden, false, "the message is shown");
  assert.strictEqual(doc.canvas.hidden, true, "the canvas is hidden");
  assert.strictEqual(doc.removed.length, 2, "the wasm loaders are removed");
}

function the_page_starts_normally_with_webgpu() {
  const doc = fakeDocument();
  assert.strictEqual(pcbcraftNeedsWebGpu({ gpu: {} }, doc), false);
  assert.strictEqual(doc.notice.hidden, true);
  assert.strictEqual(doc.removed.length, 0);
}

the_message_shows_without_webgpu();
the_page_starts_normally_with_webgpu();
console.log("ok");
