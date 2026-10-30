// Runs in the document head before Trunk injects the wasm loader. Without
// WebGPU or WebGL2 the wasm cannot draw, so show the notice and drop loaders
// that appear after this script. The text must equal catalog key
// web.needs_webgpu (checked by a test in src/main.rs).
function pcbcraftHasWebGpu(nav) {
  return !!(nav && nav.gpu);
}

function pcbcraftHasWebGl2(doc) {
  var canvas = doc.createElement("canvas");
  if (!canvas || !canvas.getContext) {
    return false;
  }
  return !!(
    canvas.getContext("webgl2") || canvas.getContext("experimental-webgl2")
  );
}

function pcbcraftGraphicsUsable(nav, doc) {
  return pcbcraftHasWebGpu(nav) || pcbcraftHasWebGl2(doc);
}

function pcbcraftShowGraphicsNotice(doc) {
  var notice = doc.getElementById("needs-webgpu");
  if (notice) {
    notice.hidden = false;
  }
  var canvas = doc.getElementById("pcbcraft");
  if (canvas) {
    canvas.hidden = true;
  }
}

function pcbcraftIsWasmLoader(node) {
  if (!node || node.nodeType !== 1) {
    return false;
  }
  var tag = node.tagName;
  if (tag === "SCRIPT") {
    return node.type === "module";
  }
  if (tag === "LINK") {
    var rel = node.rel || "";
    if (rel === "modulepreload") {
      return true;
    }
    if (rel === "preload") {
      var asKind = node.getAttribute("as");
      return asKind === "fetch" || asKind === "script";
    }
    if (node.getAttribute("data-trunk") !== null && rel === "rust") {
      return true;
    }
  }
  return false;
}

function pcbcraftRemoveWasmLoaders(doc, fromNode) {
  var loaders = doc.querySelectorAll(
    'script[type="module"], link[rel="modulepreload"], link[rel="preload"], link[data-trunk][rel="rust"]'
  );
  for (var i = 0; i < loaders.length; i += 1) {
    loaders[i].remove();
  }
  var node = fromNode && fromNode.nextSibling;
  while (node) {
    var next = node.nextSibling;
    if (pcbcraftIsWasmLoader(node)) {
      node.remove();
    }
    node = next;
  }
}

// Returns true when wasm must not load (no usable graphics API).
function pcbcraftNeedsWebGpu(nav, doc) {
  if (pcbcraftGraphicsUsable(nav, doc)) {
    return false;
  }
  pcbcraftShowGraphicsNotice(doc);
  pcbcraftRemoveWasmLoaders(
    doc,
    typeof document !== "undefined" ? document.currentScript : null
  );
  return true;
}

if (typeof module !== "undefined") {
  module.exports = {
    pcbcraftNeedsWebGpu: pcbcraftNeedsWebGpu,
    pcbcraftHasWebGpu: pcbcraftHasWebGpu,
    pcbcraftHasWebGl2: pcbcraftHasWebGl2,
  };
}

function pcbcraftWatchBlockedLoaders(doc) {
  pcbcraftRemoveWasmLoaders(doc, null);
  if (typeof MutationObserver === "undefined") {
    return;
  }
  var observer = new MutationObserver(function () {
    pcbcraftRemoveWasmLoaders(doc, null);
  });
  observer.observe(doc.documentElement, { childList: true, subtree: true });
}

if (typeof document !== "undefined" && typeof window !== "undefined") {
  var blocked = pcbcraftNeedsWebGpu(navigator, document);
  if (blocked) {
    pcbcraftWatchBlockedLoaders(document);
  } else {
    window.addEventListener("error", function () {
      pcbcraftShowGraphicsNotice(document);
    });
    window.addEventListener("unhandledrejection", function () {
      pcbcraftShowGraphicsNotice(document);
    });
  }
}
