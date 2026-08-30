const HEADER_BYTES = 64;
const RECORD_SIZE = 48;
let ring;
let chunks = [];
let recordCount = 0;
let drainCount = 0;

function drain() {
  if (!ring) return;
  const write = Atomics.load(ring.header, 2) >>> 0;
  let read = Atomics.load(ring.header, 3) >>> 0;
  let available = (write - read) >>> 0;
  while (available > 0) {
    const slot = read % ring.capacity;
    const count = Math.min(available, ring.capacity - slot);
    chunks.push(ring.data.slice(slot * RECORD_SIZE, (slot + count) * RECORD_SIZE));
    recordCount += count;
    read = (read + count) >>> 0;
    available -= count;
  }
  Atomics.store(ring.header, 3, read | 0);
  drainCount++;
}

self.onmessage = async event => {
  const message = event.data;
  if (message.type === "start") {
    const header = new Int32Array(message.sab, 0, 16);
    ring = {
      header,
      data: new Uint8Array(message.sab, HEADER_BYTES),
      capacity: Atomics.load(header, 1),
    };
    self.postMessage({type: "ready"});
    return;
  }
  if (message.type === "drain") {
    drain();
    return;
  }
  if (message.type !== "finish") return;
  try {
    drain();
    const module = await import("./pkg/collector/perfetto_everywhere_collector.js");
    await module.default();
    const collector = new module.WasmCollector(2_000_000);
    collector.register_realm(4, "AudioWorklet", BigInt(message.sampleRate));
    for (const metadata of message.metadata) {
      collector.register_metadata(metadata.id, metadata.namespace, metadata.label);
    }
    for (const sample of message.calibrations) {
      collector.add_calibration(
        4,
        104,
        sample.sourceFrame,
        sample.referenceNs,
        sample.uncertaintyNs,
      );
    }
    for (const chunk of chunks) collector.ingest_batch(chunk);
    collector.set_health(
      4,
      BigInt(recordCount),
      BigInt(Atomics.load(ring.header, 4) >>> 0),
      BigInt(drainCount),
      Atomics.load(ring.header, 8) >>> 0,
      0n,
    );
    const trace = collector.finish();
    self.postMessage({
      type: "trace",
      trace,
      records: recordCount,
      callbacks: Atomics.load(ring.header, 5) >>> 0,
      dropped: Atomics.load(ring.header, 4) >>> 0,
      discontinuities: Atomics.load(ring.header, 7) >>> 0,
      highWater: Atomics.load(ring.header, 8) >>> 0,
      drains: drainCount,
    }, [trace.buffer]);
  } catch (error) {
    self.postMessage({type: "error", error: error.stack || String(error)});
  }
};
