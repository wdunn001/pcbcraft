// Browser smoke test for the built web app. Serves a trunk `dist` folder on
// 127.0.0.1, loads it in headless Chromium over the DevTools pipe (no npm
// packages), and fails on any console error, uncaught exception or panic. It
// then checks the first frame painted something (the page screenshot is not
// one flat colour), that the Layout tab switches (clicks it at a fixed spot), and that the crash notice
// stays hidden.
//
// Usage: node web-smoke.mjs <dist-dir> [--shots <dir>] [--expect blank]
// Environment: CHROME=/path/to/chromium (default: first of google-chrome,
// chromium, chromium-browser on PATH).
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { extname, join, normalize } from "node:path";
import { inflateSync } from "node:zlib";

const args = process.argv.slice(2);
const dist = args[0];
const option = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : null);
const shots = option("--shots");
if (!dist) {
  console.error("usage: web-smoke.mjs <dist-dir> [--shots <dir>]");
  process.exit(2);
}

const failures = [];
const fail = (message) => {
  failures.push(message);
  console.error("FAIL: " + message);
};

const types = {
  ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".css": "text/css", ".svg": "image/svg+xml", ".png": "image/png", ".ico": "image/x-icon",
  ".webmanifest": "application/manifest+json", ".json": "application/json",
};
const server = createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, "http://x").pathname));
  const file = join(dist, path === "/" ? "index.html" : path);
  if (!file.startsWith(join(dist))) {
    response.writeHead(403).end();
    return;
  }
  try {
    const body = await readFile(file);
    response.writeHead(200, { "content-type": types[extname(file)] || "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const url = `http://127.0.0.1:${server.address().port}/`;

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    try {
      return execFileSync("which", [name], { encoding: "utf8" }).trim();
    } catch {}
  }
  console.error("no Chromium or Chrome found; set CHROME");
  process.exit(2);
}
const userData = join(process.env.TMPDIR || "/tmp", `pcbcraft-smoke-${process.pid}`);
const chrome = spawn(findChrome(), [
  "--headless=new", "--no-sandbox", "--disable-dev-shm-usage", "--remote-debugging-pipe",
  `--user-data-dir=${userData}`, "--window-size=1400,900",
  // Software rendering: no GPU on a CI machine. WebGL 2 through SwiftShader.
  "--enable-unsafe-swiftshader", "--use-angle=swiftshader",
  "--no-first-run", "about:blank",
], { stdio: ["ignore", "ignore", "ignore", "pipe", "pipe"] });

// DevTools pipe protocol: JSON messages separated by NUL bytes.
let nextId = 1;
const waiting = new Map();
const listeners = [];
let buffer = Buffer.alloc(0);
chrome.stdio[4].on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  let at;
  while ((at = buffer.indexOf(0)) >= 0) {
    const message = JSON.parse(buffer.subarray(0, at).toString());
    buffer = buffer.subarray(at + 1);
    if (message.id && waiting.has(message.id)) {
      const { resolve, reject } = waiting.get(message.id);
      waiting.delete(message.id);
      message.error ? reject(new Error(message.error.message)) : resolve(message.result);
    } else {
      listeners.forEach((listener) => listener(message));
    }
  }
});
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    waiting.set(id, { resolve, reject });
    chrome.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + "\0");
  });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Minimal PNG reader for the 8-bit RGBA/RGB screenshots Chromium produces.
function decodePng(png) {
  let at = 8, width = 0, height = 0, channels = 4;
  const idat = [];
  while (at < png.length) {
    const length = png.readUInt32BE(at);
    const kind = png.toString("latin1", at + 4, at + 8);
    const data = png.subarray(at + 8, at + 8 + length);
    if (kind === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      channels = data[9] === 6 ? 4 : 3;
    } else if (kind === "IDAT") idat.push(data);
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const pixels = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const value = raw[y * (stride + 1) + 1 + x];
      const left = x >= channels ? pixels[y * stride + x - channels] : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const upLeft = y > 0 && x >= channels ? pixels[(y - 1) * stride + x - channels] : 0;
      let predicted = 0;
      if (filter === 1) predicted = left;
      else if (filter === 2) predicted = up;
      else if (filter === 3) predicted = (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const [a, b, c] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - upLeft)];
        predicted = a <= b && a <= c ? left : b <= c ? up : upLeft;
      }
      pixels[y * stride + x] = (value + predicted) & 255;
    }
  }
  return { width, height, channels, pixels };
}

// How much of the page is not the page's own background colour, and how many
// distinct colours it holds. A blank dark page scores zero on both.
function paintStats(image) {
  const colours = new Set();
  let painted = 0;
  const total = image.width * image.height;
  for (let i = 0; i < total; i++) {
    const o = i * image.channels;
    const [r, g, b] = [image.pixels[o], image.pixels[o + 1], image.pixels[o + 2]];
    colours.add((r << 16) | (g << 8) | b);
    if (!(Math.abs(r - 0x14) < 3 && Math.abs(g - 0x1d) < 3 && Math.abs(b - 0x2b) < 3)) painted++;
  }
  return { painted: painted / total, colours: colours.size };
}

function difference(a, b) {
  let changed = 0;
  const total = a.width * a.height;
  for (let i = 0; i < total; i++) {
    const o = i * a.channels;
    if (Math.abs(a.pixels[o] - b.pixels[o]) + Math.abs(a.pixels[o + 1] - b.pixels[o + 1]) > 24) changed++;
  }
  return changed / total;
}

let step = 0;
async function shoot(session, label) {
  const { data } = await send("Page.captureScreenshot", { format: "png" }, session);
  const png = Buffer.from(data, "base64");
  if (shots) {
    await mkdir(shots, { recursive: true });
    await writeFile(join(shots, `${String(++step).padStart(2, "0")}-${label}.png`), png);
  }
  return decodePng(png);
}

async function click(session, x, y) {
  const base = { x, y, button: "left", clickCount: 1 };
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y }, session);
  await send("Input.dispatchMouseEvent", { type: "mousePressed", ...base }, session);
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", ...base }, session);
}

const consoleProblems = [];
try {
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  listeners.push((message) => {
    if (message.sessionId !== sessionId) return;
    const { method, params } = message;
    if (method === "Runtime.consoleAPICalled" && params.type === "error") {
      consoleProblems.push("console.error: " + params.args.map((a) => a.value ?? a.description ?? "").join(" "));
    } else if (method === "Runtime.exceptionThrown") {
      const d = params.exceptionDetails;
      consoleProblems.push("exception: " + (d.exception?.description || d.text));
    } else if (method === "Log.entryAdded" && params.entry.level === "error") {
      consoleProblems.push("log error: " + params.entry.text + " " + (params.entry.url || ""));
    }
  });
  await send("Runtime.enable", {}, sessionId);
  await send("Log.enable", {}, sessionId);
  await send("Page.enable", {}, sessionId);
  await send("Emulation.setDeviceMetricsOverride",
    { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false }, sessionId);
  await send("Page.navigate", { url }, sessionId);

  // First frame: poll until the page holds more than a flat background.
  let first = null;
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    await sleep(1500);
    const image = await shoot(sessionId, "poll");
    const stats = paintStats(image);
    if (stats.painted > 0.3 && stats.colours > 30) { first = { image, stats }; break; }
    if (consoleProblems.some((p) => /panicked|unreachable/i.test(p))) break;
  }
  const evaluate = async (expression) =>
    (await send("Runtime.evaluate", { expression, returnByValue: true }, sessionId)).result.value;
  if (!first) {
    fail("the first frame never painted (page stayed blank or flat)");
  } else {
    console.log(`first frame painted: ${(first.stats.painted * 100).toFixed(0)}% non-background, ${first.stats.colours} colours`);
    const crashed = await evaluate(`!document.getElementById("web-crashed").hidden`);
    if (crashed) fail("the crash notice is showing");
    // The tab strip is at the top of the 1400x900 window: Files, Schematic,
    // Layout, Manufacture. Layout sits near x=199, y=54; a real switch redraws
    // most of the window (board instead of sheet, different panels).
    const before = await shoot(sessionId, "schematic");
    await click(sessionId, 199, 54);
    await sleep(2000);
    const after = await shoot(sessionId, "layout");
    const changed = difference(before, after);
    console.log(`layout tab: ${(changed * 100).toFixed(1)}% of pixels changed`);
    if (changed < 0.03) fail("clicking the Layout tab did not change the picture (tab did not switch)");
    if (paintStats(after).painted < 0.3) fail("the Layout tab painted a blank window");
  }
  // A browser with neither WebGPU nor WebGL 2: the notice must show and the
  // wasm must not be downloaded.
  {
    const second = await send("Target.createTarget", { url: "about:blank" });
    const session2 = (await send("Target.attachToTarget", { targetId: second.targetId, flatten: true })).sessionId;
    const requested = [];
    listeners.push((message) => {
      if (message.sessionId === session2 && message.method === "Network.requestWillBeSent") {
        requested.push(message.params.request.url);
      }
    });
    await send("Network.enable", {}, session2);
    await send("Page.enable", {}, session2);
    await send("Page.addScriptToEvaluateOnNewDocument", {
      source: "HTMLCanvasElement.prototype.getContext = () => null; Object.defineProperty(navigator, 'gpu', { value: undefined });",
    }, session2);
    await send("Page.navigate", { url }, session2);
    await sleep(4000);
    const shown = (await send("Runtime.evaluate",
      { expression: `!document.getElementById("needs-webgpu").hidden`, returnByValue: true }, session2)).result.value;
    if (!shown) fail("a browser with no WebGPU and no WebGL 2 did not see the notice");
    if (requested.some((u) => u.endsWith(".wasm"))) fail("the wasm was downloaded in a browser with no graphics");
    console.log(`no-graphics check: notice ${shown ? "shown" : "missing"}, ${requested.length} requests, wasm fetched: ${requested.some((u) => u.endsWith(".wasm"))}`);
  }
  await sleep(500);
  if (consoleProblems.length) consoleProblems.forEach((p) => fail(p.slice(0, 400)));
} catch (error) {
  fail("smoke test error: " + (error && error.stack || error));
} finally {
  chrome.kill("SIGTERM");
  server.close();
}
if (failures.length) {
  console.error(`${failures.length} problem(s)`);
  process.exit(1);
}
console.log("web smoke passed");
process.exit(0);
