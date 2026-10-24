// Minimal WebView2 DevTools client for evidence capture.
//   node tooling/cdp.mjs eval "<javascript>"
//   node tooling/cdp.mjs shot <file.png> [light|dark]
//   node tooling/cdp.mjs reload
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222

import { writeFileSync } from "node:fs";

const port = process.env.CDP_PORT || "9222";
const [, , command, ...rest] = process.argv;

async function pageTarget() {
  const response = await fetch(`http://127.0.0.1:${port}/json`);
  const targets = await response.json();
  return (
    targets.find((target) => target.type === "page" && /index\.html|tauri/.test(target.url || "")) ||
    targets.find((target) => target.type === "page")
  );
}

const target = await pageTarget();
if (!target) {
  console.error("no page target");
  process.exit(1);
}

const socket = new WebSocket(target.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();

socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) reject(new Error(message.error.message));
    else resolve(message.result);
  }
});

await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});

function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text || "evaluation failed");
  }
  return result.result ? result.result.value : undefined;
}

if (command === "eval") {
  const value = await evaluate(rest[0]);
  console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
} else if (command === "eval-light" || command === "eval-dark") {
  await send("Emulation.setEmulatedMedia", {
    features: [
      {
        name: "prefers-color-scheme",
        value: command === "eval-light" ? "light" : "dark",
      },
    ],
  });
  const value = await evaluate(rest[0]);
  console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
} else if (command === "reload") {
  await send("Page.reload", { ignoreCache: true });
} else if (command === "shot") {
  const [file, scheme = "dark"] = rest;
  await send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: scheme }],
  });
  await send("Emulation.setDeviceMetricsOverride", {
    width: Number(process.env.SHOT_WIDTH || 1180),
    height: Number(process.env.SHOT_HEIGHT || 780),
    deviceScaleFactor: 1,
    mobile: false,
  });
  await evaluate("document.fonts.ready.then(() => true)");
  // Theme switches animate the themed backgrounds; capture after the 150 ms
  // transition instead of the first frame.
  await new Promise((resolve) => setTimeout(resolve, 400));
  const shot = await send("Page.captureScreenshot", { format: "png", fromSurface: true });
  writeFileSync(file, Buffer.from(shot.data, "base64"));
  console.log(file);
} else {
  console.error("usage: eval <js> | shot <file> [light|dark] | reload");
  process.exit(1);
}

socket.close();
