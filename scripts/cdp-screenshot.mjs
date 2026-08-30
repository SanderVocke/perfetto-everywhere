#!/usr/bin/env node
import { writeFile } from "node:fs/promises";

const [port, output, delaySeconds = "20", clickX, clickY] = process.argv.slice(2);
if (!port || !output) {
  throw new Error("usage: cdp-screenshot.mjs PORT OUTPUT [DELAY_SECONDS]");
}
await new Promise(resolve => setTimeout(resolve, Number(delaySeconds) * 1000));
const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json());
const page = targets.find(target => target.type === "page");
if (!page) throw new Error("no Chromium page target");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.addEventListener("open", resolve, { once: true });
  ws.addEventListener("error", reject, { once: true });
});
let nextId = 1;
function command(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const listener = event => {
      const message = JSON.parse(event.data);
      if (message.id !== id) return;
      ws.removeEventListener("message", listener);
      if (message.error) reject(new Error(JSON.stringify(message.error)));
      else resolve(message.result);
    };
    ws.addEventListener("message", listener);
    ws.send(JSON.stringify({ id, method, params }));
  });
}
if (clickX && clickY) {
  const point = { x: Number(clickX), y: Number(clickY), button: "left", clickCount: 1 };
  await command("Input.dispatchMouseEvent", { type: "mousePressed", ...point });
  await command("Input.dispatchMouseEvent", { type: "mouseReleased", ...point });
  await new Promise(resolve => setTimeout(resolve, 2000));
}
const result = await command("Page.captureScreenshot", { format: "png", fromSurface: true });
await writeFile(output, Buffer.from(result.data, "base64"));
ws.close();
