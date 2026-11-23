// The browser driver of scripts/web-run.sh (D427). Serves target/web/<package>/ on 127.0.0.1, opens
// it in the separate Chromium (headless, a throwaway profile, page.screenshot() only), reads the app's
// console lines and judges them. Exit codes: 0 passed, 1 a check failed (the line is quoted), 2 usage
// or setup. The browser is killed on every exit path.
//
//   node run.mjs --package <name> --build-dir <dir> --out <dir> [--demo] [--gl] [--smoke] [--keep-serving]
//   node run.mjs --self-test       (the server's path rules and the blank-shot check; no browser)
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { execFileSync } from "node:child_process";

const SERVER_HOST = "127.0.0.1";
const PRESENTED_MS = 30_000; // `presented 1` within this
const TOUR_MS = 60_000 + 60_000; // the tour's own length plus a minute of slack
const HARD_CAP_MS = 240_000; // the whole run
const MAX_SHOTS = 5;
const MIN_COLOURS = 32; // a shot needs more distinct colours than this
// The browser itself runs at device scale 2 (`--force-device-scale-factor`, so winit's
// `devicePixelContentBoxSize` is the real 2x; an emulated scale leaves it at 1x) in a window bigger
// than the demo's 402x874 canvas, which the shots clip to.
const WINDOW = { width: 600, height: 1100 };
const SCALE = 2;
const MARKS = 16;

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".css": "text/css; charset=utf-8",
  ".png": "image/png",
  ".json": "application/json",
  ".txt": "text/plain; charset=utf-8",
  ".ttf": "font/ttf",
};

function usage(why) {
  process.stderr.write(`web-run: ${why}\n`);
  process.exit(2);
}

// ---------------------------------------------------------------------------------------------
// The static server: GET and HEAD of regular files under one directory, loopback only.

/** The file under `rootReal` a request target names, or null (every refusal answers 404). */
function resolveTarget(rootReal, target) {
  const raw = target.split("#")[0].split("?")[0];
  if (!raw.startsWith("/")) return null; // an absolute-form target (`http://host/..`) or garbage
  let decoded;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    return null;
  }
  if (decoded.includes("\0") || decoded.includes("\\")) return null;
  const segments = decoded.split("/").slice(1);
  if (segments[segments.length - 1] === "") segments[segments.length - 1] = "index.html";
  // No empty segment (`//etc/passwd`), no `.` and no `..`, whatever the encoding.
  if (segments.some((s) => s === "" || s === "." || s === "..")) return null;
  const candidate = path.join(rootReal, ...segments);
  let real;
  try {
    real = fs.realpathSync(candidate);
  } catch {
    return null;
  }
  if (real !== rootReal && !real.startsWith(rootReal + path.sep)) return null; // a symlink out
  try {
    if (!fs.statSync(real).isFile()) return null;
  } catch {
    return null;
  }
  return real;
}

/** Starts the server over `root`; resolves with `{ server, port, close }`. */
function serve(root) {
  const rootReal = fs.realpathSync(root);
  return new Promise((resolve, reject) => {
    let port = 0;
    const server = http.createServer((req, res) => {
      const refuse = (code) => {
        res.writeHead(code, { "content-type": "text/plain" });
        res.end(code === 404 ? "not found\n" : "refused\n");
      };
      // A page of another origin (DNS rebinding) names another host.
      if (req.headers.host !== `${SERVER_HOST}:${port}`) return refuse(403);
      if (req.method !== "GET" && req.method !== "HEAD") return refuse(405);
      const file = resolveTarget(rootReal, req.url ?? "");
      if (file === null) return refuse(404);
      const type = TYPES[path.extname(file).toLowerCase()] ?? "application/octet-stream";
      const data = fs.readFileSync(file);
      res.writeHead(200, {
        "content-type": type,
        "content-length": data.length,
        "cache-control": "no-store",
      });
      res.end(req.method === "HEAD" ? undefined : data);
    });
    server.on("error", reject);
    server.listen(0, SERVER_HOST, () => {
      port = server.address().port;
      resolve({ server, port, close: () => new Promise((r) => server.close(r)) });
    });
  });
}

// ---------------------------------------------------------------------------------------------
// The blank-shot check: PNG decoded here (zlib only) to count distinct colours.

/** The number of distinct pixel colours in an 8-bit RGB or RGBA non-interlaced PNG, counted up to `cap`. */
function distinctColours(png, cap) {
  if (png.length < 8 || png.toString("latin1", 1, 4) !== "PNG") throw new Error("not a PNG");
  let pos = 8;
  let ihdr = null;
  const idat = [];
  while (pos + 8 <= png.length) {
    const len = png.readUInt32BE(pos);
    const type = png.toString("latin1", pos + 4, pos + 8);
    const body = png.subarray(pos + 8, pos + 8 + len);
    if (type === "IHDR") ihdr = body;
    else if (type === "IDAT") idat.push(body);
    else if (type === "IEND") break;
    pos += 12 + len;
  }
  if (!ihdr) throw new Error("PNG without IHDR");
  const width = ihdr.readUInt32BE(0);
  const height = ihdr.readUInt32BE(4);
  const depth = ihdr[8];
  const colorType = ihdr[9];
  const interlace = ihdr[12];
  if (depth !== 8 || interlace !== 0 || (colorType !== 2 && colorType !== 6)) {
    throw new Error(`unsupported PNG (depth ${depth}, colour type ${colorType}, interlace ${interlace})`);
  }
  const bpp = colorType === 6 ? 4 : 3;
  const stride = width * bpp;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  if (raw.length !== (stride + 1) * height) throw new Error("PNG data of the wrong length");
  const rows = [Buffer.alloc(stride), Buffer.alloc(stride)];
  const seen = new Set();
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const cur = rows[y & 1];
    const up = rows[(y & 1) ^ 1];
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? cur[x - bpp] : 0;
      const b = y > 0 ? up[x] : 0;
      const c = x >= bpp && y > 0 ? up[x - bpp] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      } else if (filter !== 0) throw new Error(`PNG filter ${filter}`);
      cur[x] = v & 255;
    }
    for (let x = 0; x < stride; x += bpp) {
      seen.add((cur[x] << 16) | (cur[x + 1] << 8) | cur[x + 2]);
      if (seen.size > cap) return seen.size;
    }
  }
  return seen.size;
}

/** A PNG of `width` x `height` whose pixel (x, y) is `rgb(x, y)` = [r, g, b]; for the self-test. */
function makePng(width, height, rgb) {
  const crcTable = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
  }
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const byte of buf) c = crcTable[(c ^ byte) & 255] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, body) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(body.length, 0);
    head.write(type, 4, "latin1");
    const tail = Buffer.alloc(4);
    tail.writeUInt32BE(crc(Buffer.concat([head.subarray(4), body])), 0);
    return Buffer.concat([head, body, tail]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const [r, g, b] = rgb(x, y);
      const i = y * (width * 3 + 1) + 1 + x * 3;
      raw[i] = r;
      raw[i + 1] = g;
      raw[i + 2] = b;
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---------------------------------------------------------------------------------------------
// --self-test: the server's path rules and the blank-shot check, each against known answers.

function rawGet(port, target, host = `${SERVER_HOST}:${port}`) {
  return new Promise((resolve, reject) => {
    // Node's client sends the target as given: no `..` is folded away.
    const req = http.request({ host: SERVER_HOST, port, path: target, method: "GET", headers: { host } }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve({ status: res.statusCode, type: res.headers["content-type"], body: Buffer.concat(chunks) }));
    });
    req.on("error", reject);
    req.end();
  });
}

async function selfTest(scratchRoot) {
  fs.mkdirSync(scratchRoot, { recursive: true });
  const base = fs.mkdtempSync(path.join(scratchRoot, "selftest-"));
  const root = path.join(base, "site");
  fs.mkdirSync(path.join(root, "sub"), { recursive: true });
  fs.writeFileSync(path.join(root, "index.html"), "<p>index</p>");
  fs.writeFileSync(path.join(root, "app.js"), "export {};");
  fs.writeFileSync(path.join(root, "app_bg.wasm"), Buffer.from([0, 0x61, 0x73, 0x6d]));
  fs.writeFileSync(path.join(root, "sub", "inner.txt"), "inner");
  fs.writeFileSync(path.join(base, "secret.txt"), "SECRET");
  fs.symlinkSync(path.join(base, "secret.txt"), path.join(root, "link.txt"));
  const failures = [];
  const expect = (name, ok) => {
    process.stdout.write(`self-test ${ok ? "ok  " : "FAIL"} ${name}\n`);
    if (!ok) failures.push(name);
  };
  const { port, close } = await serve(root);
  try {
    const get = (t) => rawGet(port, t);
    let r = await get("/");
    expect("/ serves index.html as text/html", r.status === 200 && r.type.startsWith("text/html") && r.body.toString() === "<p>index</p>");
    r = await get("/app.js");
    expect(".js is text/javascript", r.status === 200 && r.type.startsWith("text/javascript"));
    r = await get("/app_bg.wasm");
    expect(".wasm is application/wasm", r.status === 200 && r.type === "application/wasm");
    r = await get("/sub/inner.txt");
    expect("a file in a subdirectory is served", r.status === 200 && r.body.toString() === "inner");
    r = await get("/app.js?x=1");
    expect("a query string is ignored", r.status === 200);
    for (const bad of [
      "/../secret.txt",
      "/sub/../../secret.txt",
      "/%2e%2e/secret.txt",
      "/..%2fsecret.txt",
      "/%2E%2E%2Fsecret.txt",
      "/sub/%2e%2e/%2e%2e/secret.txt",
      "//etc/passwd",
      "/etc/passwd",
      "/..\\secret.txt",
      "/%5c..%5csecret.txt",
      "/app.js%00.png",
      "/link.txt",
      "/sub",
      "/missing",
      "http://127.0.0.1/../secret.txt",
      "*",
    ]) {
      r = await get(bad);
      expect(`${JSON.stringify(bad)} answers 404 and never the secret`, r.status === 404 && !r.body.toString().includes("SECRET"));
    }
    r = await rawGet(port, "/index.html", "evil.example:80");
    expect("a foreign Host header answers 403", r.status === 403);
    const post = await new Promise((resolve, reject) => {
      const req = http.request({ host: SERVER_HOST, port, path: "/", method: "POST", headers: { host: `${SERVER_HOST}:${port}` } }, (res) => {
        res.resume();
        resolve(res.statusCode);
      });
      req.on("error", reject);
      req.end();
    });
    expect("POST answers 405", post === 405);
  } finally {
    await close();
  }
  // The blank-shot check: a flat PNG must fail it, a gradient must pass it.
  const flat = makePng(64, 64, () => [20, 30, 40]);
  const colours = distinctColours(flat, MIN_COLOURS);
  expect(`a flat PNG has ${colours} colour(s), under the ${MIN_COLOURS} the check needs`, colours <= MIN_COLOURS);
  const few = makePng(64, 64, (x) => [x % 8 === 0 ? 255 : 0, 0, 0]);
  expect("a two-colour PNG fails the check", distinctColours(few, MIN_COLOURS) <= MIN_COLOURS);
  const grad = makePng(64, 64, (x, y) => [x * 4, y * 4, (x + y) * 2]);
  const gradColours = distinctColours(grad, MIN_COLOURS);
  expect(`a gradient PNG has more than ${MIN_COLOURS} colours (${gradColours})`, gradColours > MIN_COLOURS);
  fs.rmSync(base, { recursive: true, force: true }); // made above by mkdtemp: ours
  if (failures.length > 0) {
    process.stdout.write(`web-run: self-test FAILED (${failures.length})\n`);
    process.exit(1);
  }
  process.stdout.write("web-run: self-test passed\n");
}

// ---------------------------------------------------------------------------------------------
// The browser run.

function parseArgs(argv) {
  const opts = { demo: false, gl: false, smoke: false, keepServing: false, selfTest: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => {
      if (i + 1 >= argv.length) usage(`${a} needs a value`);
      return argv[++i];
    };
    if (a === "--package") opts.package = val();
    else if (a === "--build-dir") opts.buildDir = val();
    else if (a === "--out") opts.out = val();
    else if (a === "--chromium") opts.chromium = val();
    else if (a === "--demo") opts.demo = true;
    else if (a === "--gl") opts.gl = true;
    else if (a === "--smoke") opts.smoke = true;
    else if (a === "--keep-serving") opts.keepServing = true;
    else if (a === "--self-test") opts.selfTest = true;
    else usage(`unknown argument ${a}`);
  }
  return opts;
}

/** Refuses the user's own browsers: never launched by a script. */
function checkBrowserPath(p) {
  let real = p;
  try {
    real = fs.realpathSync(p);
  } catch {
    usage(`no Chromium at ${p} (set LUNEUI_CHROMIUM)`);
  }
  for (const candidate of [p, real]) {
    const lower = candidate.toLowerCase();
    if (lower.includes("/applications/google chrome.app") || lower.includes("/applications/firefox.app")) {
      usage(`refusing to launch ${candidate}: the user's own browsers are never driven by a script`);
    }
  }
  try {
    fs.accessSync(real, fs.constants.X_OK);
  } catch {
    usage(`${real} is not executable`);
  }
  return real;
}

// ---------------------------------------------------------------------------------------------
// --smoke: the web text bridge (D429). The page's search field is clicked, typed into, composed into,
// tabbed out of and clicked away from; each step is judged on the demo's own `probe` lines.

/** The text the demo's `probe field` lines last printed, `{:?}`-decoded for the plain ASCII and CJK this check types. */
function lastField(lines) {
  for (let i = lines.length - 1; i >= 0; i--) {
    const m = /^probe field "(.*)"$/.exec(lines[i]);
    if (m) return m[1];
  }
  return null;
}

async function smokeTextInput(page, context, lines, waitLine) {
  const count = (re) => lines.filter((l) => re.test(l)).length;
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const expectField = async (want, what) => {
    await waitLine(new RegExp(`^probe field "${want}"$`), 5_000, `\`probe field "${want}"\` (${what})`);
    await sleep(400); // a doubled character would show as a later line
    const got = lastField(lines);
    if (got !== want) throw new Error(`${what}: the field is ${JSON.stringify(got)}, wanted ${JSON.stringify(want)}`);
  };
  const stays = async (what) => {
    const before = lastField(lines);
    await sleep(500);
    if (lastField(lines) !== before) throw new Error(`${what}: the field changed to ${JSON.stringify(lastField(lines))}`);
  };
  const active = () => page.evaluate(() => document.activeElement?.getAttribute("data-luneui") ?? document.activeElement?.tagName);

  // One bridge element; an attach adds one and its drop removes it (so a second attach does not double keys).
  const counts = await waitLine(/^probe text_input_elements /, 5_000, "`probe text_input_elements`");
  if (counts !== "probe text_input_elements 1 2 1") throw new Error(`${counts} (wanted 1 2 1: one bridge, two while a second is attached, one after its drop)`);

  const target = (await waitLine(/^probe search -?\d+,-?\d+$/, 5_000, "`probe search`")).slice("probe search ".length).split(",").map(Number);
  const box = await page.locator("#luneui").boundingBox();
  if (!box) throw new Error("the page has no canvas to click");
  const at = { x: box.x + target[0] / SCALE, y: box.y + target[1] / SCALE };

  const focusedBefore = count(/^probe focused /);
  await page.mouse.click(at.x, at.y);
  await waitLine(/^probe ime Enabled$/, 5_000, "`probe ime Enabled` after the click in the field");
  await sleep(500);
  if ((await active()) !== "text-input") throw new Error(`after the click the focused element is ${await active()}, not the text bridge`);
  const first = count(/^probe focused /) - focusedBefore;
  if (first > 1) throw new Error(`${first} Focused changes reached the translator for one click (at most 1)`);
  // A second click in the field: the canvas takes the focus on press and gives it back on release.
  const before2 = count(/^probe focused /);
  await page.mouse.click(at.x, at.y);
  await sleep(500);
  const second = count(/^probe focused /) - before2;
  if (second > 1) throw new Error(`${second} Focused changes reached the translator for the second click (at most 1)`);
  if ((await active()) !== "text-input") throw new Error(`after the second click the focused element is ${await active()}`);
  process.stdout.write(`web-run: field focus ok (${first} then ${second} Focused change(s) for two clicks)\n`);

  await page.keyboard.type("hello");
  await expectField("hello", "typing `hello`");
  await page.keyboard.press("Backspace");
  await expectField("hell", "Backspace");
  process.stdout.write("web-run: typing and Backspace ok (hello, hell)\n");

  // An input method's composition, then its commit.
  const cdp = await context.newCDPSession(page);
  await cdp.send("Input.imeSetComposition", { text: "にほん", selectionStart: 3, selectionEnd: 3 });
  await waitLine(/^probe ime Preedit\("にほん", Some\(\(9, 9\)\)\)$/, 5_000, "the preedit `にほん` (cursor 9 bytes)");
  await cdp.send("Input.insertText", { text: "日本" });
  await expectField("hell日本", "the composition's commit");
  process.stdout.write("web-run: composition ok (preedit にほん, commit 日本)\n");

  // A shortcut reaches the toolkit: select all, then Backspace empties the field. (The browser's
  // keymap is the PC one, so Ctrl+A on every OS.)
  await page.keyboard.press("Control+A");
  await sleep(300);
  await page.keyboard.press("Backspace");
  await expectField("", "select all then Backspace");
  process.stdout.write("web-run: shortcut ok (Ctrl+A, Backspace empties the field)\n");

  // Tab leaves the field: typing after it changes nothing.
  await page.keyboard.type("ab");
  await expectField("ab", "typing `ab`");
  await page.keyboard.press("Tab");
  await sleep(500);
  if ((await active()) === "text-input") throw new Error("after Tab the text bridge still holds the focus");
  await page.keyboard.type("xyz");
  await stays("typing after Tab");
  if (lastField(lines) !== "ab") throw new Error(`typing after Tab changed the field to ${JSON.stringify(lastField(lines))}`);
  process.stdout.write("web-run: Tab out ok (typing after it changes nothing)\n");

  // A click on the page outside the canvas is a lost focus: typing after it changes nothing.
  await page.mouse.click(at.x, at.y);
  await sleep(500);
  if ((await active()) !== "text-input") throw new Error(`the re-click left ${await active()} focused`);
  const lostBefore = count(/^probe focused \d+ false$/);
  await page.mouse.click(8, 8);
  await sleep(500);
  if (count(/^probe focused \d+ false$/) !== lostBefore + 1) throw new Error("a click outside the canvas was not reported as one lost focus");
  await page.keyboard.type("xyz");
  await stays("typing after a click outside");
  if (lastField(lines) !== "ab") throw new Error(`typing after a click outside changed the field to ${JSON.stringify(lastField(lines))}`);
  process.stdout.write("web-run: click outside ok (one lost focus, typing after it changes nothing)\n");
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.selfTest) {
    await selfTest(path.resolve(opts.out ?? path.join(os.tmpdir(), "luneui-web-run")));
    return;
  }
  if (!opts.package || !opts.buildDir || !opts.out) usage("--package, --build-dir and --out are required");
  if (!fs.existsSync(path.join(opts.buildDir, "index.html"))) usage(`no ${opts.buildDir}/index.html (run scripts/web-build.sh ${opts.package})`);
  fs.mkdirSync(opts.out, { recursive: true });
  const prefix = opts.gl ? "gl-" : "";

  const { port, close } = await serve(opts.buildDir);
  const origin = `http://${SERVER_HOST}:${port}`;
  const params = [];
  if (opts.demo) params.push("demo", "touches");
  if (opts.smoke) params.push("probe", "probe-clipboard", "probe-text-input");
  if (opts.gl) params.push("backend=gl");
  const url = `${origin}/index.html${params.length ? "?" + params.join("&") : ""}`;

  if (opts.keepServing) {
    process.stdout.write(`web-run: serving ${path.resolve(opts.buildDir)} at ${url} (Ctrl-C to stop)\n`);
    await new Promise((resolve) => {
      process.on("SIGINT", resolve);
      process.on("SIGTERM", resolve);
    });
    await close();
    return;
  }

  const chromiumPath = checkBrowserPath(opts.chromium ?? path.join(os.homedir(), "chromium/Chromium.app/Contents/MacOS/Chromium"));
  let chromium;
  try {
    ({ chromium } = await import("playwright-core"));
  } catch (e) {
    usage(`playwright-core is not installed (pnpm install --frozen-lockfile in scripts/web): ${e.message}`);
  }
  const profileRoot = path.resolve(opts.out, "..");
  fs.mkdirSync(profileRoot, { recursive: true });
  const profile = fs.mkdtempSync(path.join(profileRoot, "profile-")); // new, empty, ours
  const profileRe = profile.replace(/[.[\]()*+?^$|\\{}]/g, "\\$&");

  let context = null;
  let cleaned = false;
  const killBrowser = () => {
    // The browser and its helpers all carry this profile's path in their command line.
    try {
      execFileSync("pkill", ["-f", `--user-data-dir=${profileRe}`], { stdio: "ignore" });
    } catch {
      // none left
    }
  };
  const cleanupSync = () => {
    if (cleaned) return;
    cleaned = true;
    killBrowser();
    try {
      fs.rmSync(profile, { recursive: true, force: true }); // mkdtemp'd above
    } catch {
      // best effort
    }
  };
  process.on("exit", cleanupSync);
  for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(sig, () => {
      process.stderr.write(`web-run: ${sig}\n`);
      cleanupSync();
      process.exit(sig === "SIGINT" ? 130 : 143);
    });
  }

  const lines = []; // every console line, in order
  const waiters = [];
  let failure = null;
  const fail = (why) => {
    if (failure === null) failure = why;
    for (const w of waiters.splice(0)) w.reject(new Error(why));
  };
  const BAD = /wgpu|validation|panicked/i;
  const record = (kind, text) => {
    lines.push(text);
    if ((kind === "error" || kind === "warning") && BAD.test(text)) fail(`console ${kind}: ${text}`);
    if (/^demo mark \S+ FAIL/.test(text)) fail(text);
    for (const w of waiters.slice()) {
      if (w.re.test(text)) {
        waiters.splice(waiters.indexOf(w), 1);
        w.resolve(text);
      }
    }
  };
  /** The first line matching `re`, now or later; rejects after `ms` or on a failure. */
  const waitLine = (re, ms, what) => {
    if (failure !== null) return Promise.reject(new Error(failure));
    const seen = lines.find((l) => re.test(l));
    if (seen !== undefined) return Promise.resolve(seen);
    return new Promise((resolve, reject) => {
      const w = { re, resolve: (v) => (clearTimeout(t), resolve(v)), reject: (e) => (clearTimeout(t), reject(e)) };
      const t = setTimeout(() => {
        waiters.splice(waiters.indexOf(w), 1);
        reject(new Error(`timeout after ${ms / 1000} s waiting for ${what}`));
      }, ms);
      waiters.push(w);
    });
  };

  let shots = 0;
  const shoot = async (page, name) => {
    if (shots >= MAX_SHOTS) throw new Error("screenshot cap reached");
    shots++;
    const file = path.join(opts.out, `${prefix}${name}.png`);
    const box = await page.locator("#luneui").boundingBox();
    if (!box) throw new Error("the page has no canvas to shoot");
    const png = await page.screenshot({ path: file, type: "png", clip: box });
    const colours = distinctColours(png, MIN_COLOURS);
    if (colours <= MIN_COLOURS) throw new Error(`${file} is blank: ${colours} distinct colour(s), needs more than ${MIN_COLOURS}`);
    process.stdout.write(`web-run: ${file} (${colours}${colours > MIN_COLOURS * 4 ? "+" : ""} colours)\n`);
  };

  const hardCap = setTimeout(() => {
    process.stderr.write(`web-run: FAILED: the run took over ${HARD_CAP_MS / 1000} s\n`);
    cleanupSync();
    process.exit(1);
  }, HARD_CAP_MS);

  let exitCode = 1;
  try {
    context = await chromium.launchPersistentContext(profile, {
      executablePath: chromiumPath,
      headless: true,
      viewport: null,
      args: [
        "--use-mock-keychain",
        "--password-store=basic",
        "--enable-unsafe-webgpu",
        "--enable-features=WebGPU",
        "--ignore-gpu-blocklist",
        `--force-device-scale-factor=${SCALE}`,
        `--window-size=${WINDOW.width},${WINDOW.height}`,
      ],
    });
    // Nothing but this server is reachable: the page's one external request (the font stylesheet)
    // fails at once instead of making a run depend on the network.
    // The clipboard round trip of --smoke needs both permissions, for this server's origin only.
    await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin });
    await context.route(
      (u) => u.origin !== origin,
      (route) => route.abort("blockedbyclient"),
    );
    const page = context.pages()[0] ?? (await context.newPage());
    page.on("console", (m) => record(m.type(), m.text()));
    page.on("pageerror", (e) => {
      record("pageerror", `pageerror: ${e.message}`);
      fail(`pageerror: ${e.message}`);
    });
    await page.goto(url, { waitUntil: "load", timeout: 30_000 });

    await waitLine(/^presented 1 \d+x\d+$/, PRESENTED_MS, "`presented 1`");
    const want = opts.gl ? "Gl" : "BrowserWebGpu";
    const backend = await waitLine(/^probe adapter_backend /, 5_000, "`probe adapter_backend`");
    if (backend !== `probe adapter_backend ${want}`) throw new Error(`${backend} (wanted ${want})`);
    await new Promise((r) => setTimeout(r, 500)); // a few frames over the first one
    if (!opts.demo) await shoot(page, "shot-1");

    if (opts.smoke) {
      // `--probe` asks for frame 31 a second after frame 30 with `Repaint::At`: the browser's timer.
      await waitLine(/^presented 31 \d+x\d+$/, 20_000, "the timed `presented 31`");
      // The clipboard probe: the demo writes `luneui-probe-<pid>` through the toolkit's clipboard
      // service in frame 2 and reads it back in frame 4. The replies must match, and the page's own
      // `navigator.clipboard.readText()` must return that text (the write reached the browser).
      const written = (await waitLine(/^probe clipboard_write \S+$/, 5_000, "`probe clipboard_write`")).slice("probe clipboard_write ".length);
      const read = (await waitLine(/^probe clipboard \S+$/, 10_000, "`probe clipboard`")).slice("probe clipboard ".length);
      if (read === "unavailable") throw new Error("probe clipboard unavailable (the read was refused)");
      if (read !== written) throw new Error(`probe clipboard ${read} does not equal probe clipboard_write ${written}`);
      const inPage = await page.evaluate(() => navigator.clipboard.readText());
      if (inPage !== written) throw new Error("navigator.clipboard.readText() in the page does not return the written text");
      process.stdout.write("web-run: clipboard round trip ok (the text is the probe's own)\n");
    }

    if (opts.smoke) await smokeTextInput(page, context, lines, waitLine);

    if (opts.demo) {
      const marks = [
        ["sheet-half", "shot-1"],
        ["page-2", "shot-2"],
        ["settings", "shot-3"],
      ];
      let next = 0;
      const deadline = Date.now() + TOUR_MS;
      const endRe = /^demo end \d+\/\d+$/;
      while (!lines.some((l) => endRe.test(l))) {
        if (failure !== null) throw new Error(failure);
        if (Date.now() > deadline) throw new Error(`timeout waiting for \`demo end\` (${lines.filter((l) => /^demo mark \S+ ok$/.test(l)).length} marks ok)`);
        if (next < marks.length && lines.includes(`demo mark ${marks[next][0]} ok`)) {
          await new Promise((r) => setTimeout(r, 250)); // the frames that draw the mark
          await shoot(page, marks[next][1]);
          next++;
        }
        await new Promise((r) => setTimeout(r, 50));
      }
      if (failure !== null) throw new Error(failure);
      const end = lines.find((l) => endRe.test(l));
      const okMarks = lines.filter((l) => /^demo mark \S+ ok$/.test(l)).length;
      if (end !== `demo end ${MARKS}/${MARKS}` || okMarks !== MARKS) throw new Error(`${end} with ${okMarks} marks ok (wanted ${MARKS}/${MARKS})`);
      if (next < marks.length) throw new Error(`the tour ended before the shot at mark ${marks[next][0]}`);
    }
    // A last look: nothing failed while the checks ran.
    await new Promise((r) => setTimeout(r, 300));
    if (failure !== null) throw new Error(failure);
    exitCode = 0;
    process.stdout.write(`web-run: passed (${opts.demo ? "demo " : ""}${opts.gl ? "WebGL2" : "WebGPU"}${opts.smoke ? ", smoke" : ""})\n`);
  } catch (e) {
    process.stderr.write(`web-run: FAILED: ${failure ?? e.message}\n`);
    exitCode = 1;
  } finally {
    clearTimeout(hardCap);
    fs.writeFileSync(path.join(opts.out, `${prefix}demo.txt`), lines.join("\n") + "\n");
    try {
      if (context) await Promise.race([context.close(), new Promise((r) => setTimeout(r, 10_000))]);
    } catch {
      // killed below
    }
    cleanupSync();
    await close();
  }
  process.exit(exitCode);
}

main().catch((e) => {
  process.stderr.write(`web-run: ${e.stack ?? e}\n`);
  process.exit(2);
});
