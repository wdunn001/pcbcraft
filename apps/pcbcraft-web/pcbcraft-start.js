// Starts the app once webgpu-check.js found WebGPU or WebGL 2 (window.pcbcraftGraphics).
// Why a file: a page that allows scripts from its own origin only (no inline script) blocks the
// loader trunk would otherwise write into index.html. Trunk's script tag for this file carries the
// hashed names of the bindings and the wasm in data attributes (see Trunk.toml pattern_script).
// Rules: the wasm is fetched only when the check passed; a failed check leaves the notice visible.
// The loading view (#web-loading in index.html) shows what the start is doing: it names the stage,
// moves a bar while the wasm downloads, and is hidden by the app once it has started. Its words
// are the data attributes of that element, which equal the catalog keys web.loading.*.
const tag = document.querySelector('script[data-pcbcraft-bindings]');
const loading = document.getElementById('web-loading');
const label = document.getElementById('web-loading-status');
const fill = document.getElementById('web-loading-fill');
const note = document.getElementById('web-loading-note');

// Name the current stage in the status line.
function stage(name) {
  if (loading && label && loading.dataset[name]) {
    label.textContent = loading.dataset[name];
  }
}

// Move the bar. A fraction fills it; null leaves the sliding bar, for a download of unknown size.
function progress(fraction) {
  if (!fill) {
    return;
  }
  if (fraction === null) {
    fill.classList.remove('known');
    fill.style.width = '';
    return;
  }
  fill.classList.add('known');
  fill.style.width = Math.round(Math.min(Math.max(fraction, 0), 1) * 100) + '%';
}

// Fetch the wasm and report how much has arrived; the status moves to "compile" when the last byte is in.
// The size is known only when the server sends no content encoding: with a compressed file the bytes
// counted here are the unpacked ones and would not match the length, so the bar then slides without a figure.
async function fetchWasm(url) {
  const response = await fetch(url);
  if (!response.ok || !response.body) {
    progress(null);
    stage('compile');
    return response;
  }
  const total = Number(response.headers.get('content-length')) || 0;
  const sized = total > 0 && !response.headers.get('content-encoding');
  progress(sized ? 0 : null);
  const reader = response.body.getReader();
  let loaded = 0;
  const stream = new ReadableStream({
    async pull(controller) {
      const { done, value } = await reader.read();
      if (done) {
        progress(sized ? 1 : null);
        stage('compile');
        controller.close();
        return;
      }
      loaded += value.length;
      if (sized) {
        progress(loaded / total);
      }
      controller.enqueue(value);
    },
    cancel(reason) {
      return reader.cancel(reason);
    },
  });
  return new Response(stream, { status: response.status, headers: { 'Content-Type': 'application/wasm' } });
}

if (window.pcbcraftGraphics && tag) {
  const resolve = (name) => new URL(name, document.baseURI).href;
  // A first visit downloads the whole wasm; say that it is the slow one.
  const slow = setTimeout(() => {
    if (note) {
      note.hidden = false;
    }
  }, 6000);
  try {
    stage('download');
    const bindings = await import(resolve(tag.dataset.pcbcraftBindings));
    const wasm = await fetchWasm(resolve(tag.dataset.pcbcraftWasm));
    // The browser compiles the module as the bytes arrive; the last step is its setup and the app start.
    await bindings.default({ module_or_path: wasm });
    stage('start');
  } catch (error) {
    clearTimeout(slow);
    console.error(error);
    if (typeof pcbcraftShow === 'function') {
      pcbcraftShow(document, 'web-crashed');
    }
    throw error;
  }
  clearTimeout(slow);
}
