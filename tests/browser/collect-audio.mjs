#!/usr/bin/env node
import { writeFile } from "node:fs/promises";
const [port, output = "artifacts"] = process.argv.slice(2);
let targets;
for (let attempt = 0; attempt < 100; attempt++) {
  try {
    targets = await fetch(`http://127.0.0.1:${port}/json/list`).then(response => response.json());
    break;
  } catch { await new Promise(resolve => setTimeout(resolve, 100)); }
}
if (!targets) throw new Error("Chromium debugging endpoint did not start");
const page = targets.find(target => target.type === "page");
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, {once: true});
  socket.addEventListener("error", reject, {once: true});
});
let commandId = 0;
function command(method, params = {}) {
  const id = ++commandId;
  return new Promise((resolve, reject) => {
    const listener = event => {
      const message = JSON.parse(event.data);
      if (message.id !== id) return;
      socket.removeEventListener("message", listener);
      if (message.error) reject(new Error(JSON.stringify(message.error))); else resolve(message.result);
    };
    socket.addEventListener("message", listener);
    socket.send(JSON.stringify({id, method, params}));
  });
}
async function evaluate(expression) {
  const result = await command("Runtime.evaluate", {expression, returnByValue: true});
  return result.result.value;
}
let title = "";
for (let attempt = 0; attempt < 1800; attempt++) {
  try { title = await evaluate("document.title"); } catch {}
  if (title.startsWith("DONE") || title.startsWith("FAILED")) break;
  await new Promise(resolve => setTimeout(resolve, 100));
}
const state = JSON.parse(await evaluate(`JSON.stringify({
  title: document.title,
  summary: document.getElementById("summary")?.textContent,
  error: document.getElementById("error")?.textContent,
  trace: document.getElementById("trace")?.textContent
})`));
if (!state.title.startsWith("DONE")) throw new Error(`audio browser test failed: ${JSON.stringify(state)}`);
const summary = JSON.parse(state.summary);
await writeFile(`${output}/audio-transport.json`, JSON.stringify(summary, null, 2) + "\n");
await writeFile(`${output}/audio-transport.pftrace`, Buffer.from(state.trace, "base64"));
console.log(JSON.stringify(summary));
socket.close();
