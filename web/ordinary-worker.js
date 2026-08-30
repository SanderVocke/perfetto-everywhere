import init, { produce } from "./pkg/ordinary/ordinary_browser_producer.js";

await init();
const realm = Number(new URL(self.location.href).searchParams.get("realm"));
self.postMessage({type: "ready", realm});
self.onmessage = event => {
  if (event.data.type === "clock-ping") {
    const now = performance.now();
    self.postMessage({
      type: "clock-pong",
      realm,
      id: event.data.id,
      localMs: now,
      referenceMs: performance.timeOrigin + now,
      timeOriginMs: performance.timeOrigin,
    });
    return;
  }
  if (event.data.type !== "produce") return;
  const records = produce(realm, event.data.flow || 0n);
  self.postMessage({type: "records", realm, records}, [records.buffer]);
};
