// Runs before the browser build starts. The wasm needs WebGPU or WebGL 2 to
// draw anything. This script decides that first, shows a plain message when
// neither exists, and sets window.pcbcraftGraphics so the loader in the page
// (Trunk.toml, pattern_script) downloads the wasm only when it is true. It
// also shows a second message, with a reload button, when the running app
// crashes. The text must equal catalog keys web.needs_webgpu, web.crashed,
// web.reload and web.reload.tip (checked by a test in src/main.rs).
// Do not fetch the wasm from here.
function pcbcraftHasGraphics(nav, doc) {
  if (nav && nav.gpu) {
    return true;
  }
  try {
    var probe = doc.createElement("canvas");
    return !!(probe.getContext && probe.getContext("webgl2"));
  } catch (error) {
    return false;
  }
}
function pcbcraftShow(doc, id) {
  var notice = doc.getElementById(id);
  if (notice) {
    notice.hidden = false;
  }
  var canvas = doc.getElementById("pcbcraft");
  if (canvas) {
    canvas.hidden = true;
  }
}
function pcbcraftNeedsGraphics(nav, doc) {
  if (pcbcraftHasGraphics(nav, doc)) {
    return false;
  }
  pcbcraftShow(doc, "needs-webgpu");
  return true;
}
// A trap or panic inside the wasm surfaces as a RuntimeError (or as an
// "unreachable" message) on the window. The app also reports its own panics.
function pcbcraftIsCrash(error, message) {
  var text = String((error && error.message) || message || "");
  return (
    (error && error.name === "RuntimeError") ||
    /unreachable|memory access out of bounds|panicked/i.test(text)
  );
}
function pcbcraftWatchForCrash(win, doc) {
  win.addEventListener("error", function (event) {
    if (pcbcraftIsCrash(event.error, event.message)) {
      pcbcraftShow(doc, "web-crashed");
    }
  });
  win.addEventListener("unhandledrejection", function (event) {
    if (pcbcraftIsCrash(event.reason, "")) {
      pcbcraftShow(doc, "web-crashed");
    }
  });
  var button = doc.getElementById("web-reload");
  if (button) {
    button.addEventListener("click", function () {
      win.location.reload();
    });
  }
}
if (typeof module !== "undefined") {
  module.exports = {
    pcbcraftHasGraphics: pcbcraftHasGraphics,
    pcbcraftNeedsGraphics: pcbcraftNeedsGraphics,
    pcbcraftIsCrash: pcbcraftIsCrash,
    pcbcraftWatchForCrash: pcbcraftWatchForCrash,
    pcbcraftShow: pcbcraftShow,
  };
}
if (typeof document !== "undefined") {
  window.pcbcraftGraphics = !pcbcraftNeedsGraphics(navigator, document);
  pcbcraftWatchForCrash(window, document);
}
