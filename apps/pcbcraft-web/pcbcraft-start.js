// Starts the app once webgpu-check.js found WebGPU or WebGL 2 (window.pcbcraftGraphics).
// Why a file: a page that allows scripts from its own origin only (no inline script) blocks the
// loader trunk would otherwise write into index.html. Trunk's script tag for this file carries the
// hashed names of the bindings and the wasm in data attributes (see Trunk.toml pattern_script).
// Rules: the wasm is fetched only when the check passed; a failed check leaves the notice visible.
const tag = document.querySelector('script[data-pcbcraft-bindings]');
if (window.pcbcraftGraphics && tag) {
  const resolve = (name) => new URL(name, document.baseURI).href;
  const bindings = await import(resolve(tag.dataset.pcbcraftBindings));
  await bindings.default({ module_or_path: resolve(tag.dataset.pcbcraftWasm) });
}
