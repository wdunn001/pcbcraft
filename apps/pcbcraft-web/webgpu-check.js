// Runs before the browser build starts. Without WebGPU the wasm cannot draw
// anything, so this shows a plain message and keeps the wasm from loading.
// The text must equal catalog key web.needs_webgpu (checked by a test in
// src/main.rs). Do not fetch the wasm from here.
function pcbcraftNeedsWebGpu(nav, doc) {
  if (nav && nav.gpu) {
    return false;
  }
  var notice = doc.getElementById("needs-webgpu");
  if (notice) {
    notice.hidden = false;
  }
  var canvas = doc.getElementById("pcbcraft");
  if (canvas) {
    canvas.hidden = true;
  }
  var loaders = doc.querySelectorAll(
    'script[type="module"], link[rel="modulepreload"], link[rel="preload"]'
  );
  for (var i = 0; i < loaders.length; i += 1) {
    loaders[i].remove();
  }
  return true;
}
if (typeof module !== "undefined") {
  module.exports = { pcbcraftNeedsWebGpu: pcbcraftNeedsWebGpu };
}
if (typeof document !== "undefined") {
  pcbcraftNeedsWebGpu(navigator, document);
}
