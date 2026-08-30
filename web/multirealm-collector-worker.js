const HEADER_BYTES = 64;
const RECORD_SIZE = 48;
let audio;
const ordinary = new Map();
let audioChunks = [];
let audioRecords = 0;
let drains = 0;

function drainAudio() {
  if (!audio) return;
  const write = Atomics.load(audio.header, 2) >>> 0;
  let read = Atomics.load(audio.header, 3) >>> 0;
  let available = (write - read) >>> 0;
  while (available > 0) {
    const slot = read % audio.capacity;
    const count = Math.min(available, audio.capacity - slot);
    audioChunks.push(audio.data.slice(slot * RECORD_SIZE, (slot + count) * RECORD_SIZE));
    audioRecords += count;
    read = (read + count) >>> 0;
    available -= count;
  }
  Atomics.store(audio.header, 3, read | 0);
  drains++;
}

self.onmessage = async event => {
  const message = event.data;
  if (message.type === "start-audio") {
    const header = new Int32Array(message.sab, 0, 16);
    audio = {
      header,
      data: new Uint8Array(message.sab, HEADER_BYTES),
      capacity: Atomics.load(header, 1),
    };
    self.postMessage({type: "ready"});
    return;
  }
  if (message.type === "records") {
    const batches = ordinary.get(message.realm) || [];
    batches.push(new Uint8Array(message.records));
    ordinary.set(message.realm, batches);
    return;
  }
  if (message.type === "drain") {
    drainAudio();
    return;
  }
  if (message.type !== "finish") return;
  try {
    drainAudio();
    const module = await import("./pkg/collector/perfetto_everywhere_collector.js");
    await module.default();
    const collector = new module.WasmCollector(2_000_000);
    for (const realm of message.realms) {
      collector.register_realm(realm.id, realm.label, BigInt(realm.ticksPerSecond));
    }
    for (const metadata of message.metadata) {
      collector.register_metadata(metadata.id, metadata.namespace, metadata.label);
    }
    for (const sample of message.calibrations) {
      collector.add_calibration(
        sample.realm,
        sample.clock,
        sample.sourceTicks,
        sample.referenceNs,
        sample.uncertaintyNs,
      );
    }
    for (const [realm, batches] of ordinary) {
      let records = 0;
      for (const bytes of batches) {
        collector.ingest_batch(bytes);
        records += bytes.length / RECORD_SIZE;
      }
      collector.set_health(
        realm, BigInt(records), 0n, BigInt(batches.length), records, 0n,
      );
    }
    for (const chunk of audioChunks) collector.ingest_batch(chunk);
    collector.set_health(
      4,
      BigInt(audioRecords),
      BigInt(Atomics.load(audio.header, 4) >>> 0),
      BigInt(drains),
      Atomics.load(audio.header, 8) >>> 0,
      0n,
    );
    const trace = collector.finish();
    self.postMessage({
      type: "trace",
      trace,
      audioRecords,
      audioCallbacks: Atomics.load(audio.header, 5) >>> 0,
      dropped: Atomics.load(audio.header, 4) >>> 0,
      discontinuities: Atomics.load(audio.header, 7) >>> 0,
    }, [trace.buffer]);
  } catch (error) {
    self.postMessage({type: "error", error: error.stack || String(error)});
  }
};
