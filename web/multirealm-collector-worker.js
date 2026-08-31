const RECORD_SIZE = 48;
const ordinary = new Map();
let audioChunks = [];
let audioRecords = 0;
let audioSequence = 0;
let audioStatus = null;

self.onmessage = async event => {
  const message = event.data;
  if (message.type === "start") {
    audioChunks = [];
    audioRecords = 0;
    audioSequence = 0;
    audioStatus = null;
    self.postMessage({type: "ready"});
    return;
  }
  if (message.type === "records") {
    const batches = ordinary.get(message.realm) || [];
    batches.push(new Uint8Array(message.records));
    ordinary.set(message.realm, batches);
    return;
  }
  if (message.type === "trace-chunk") {
    if (message.sequence !== audioSequence) throw new Error("non-contiguous audio chunks");
    if (message.usedBytes <= 0 || message.usedBytes % RECORD_SIZE !== 0) {
      throw new Error("invalid audio chunk length");
    }
    audioChunks.push(new Uint8Array(message.buffer, 0, message.usedBytes).slice());
    audioRecords += message.usedBytes / RECORD_SIZE;
    audioSequence++;
    self.postMessage({
      type: "recycle", captureId: message.captureId, poolToken: message.poolToken,
      buffer: message.buffer,
    }, [message.buffer]);
    return;
  }
  if (message.type === "audio-stopped") {
    if (message.status.chunkCount !== audioSequence) throw new Error("incomplete audio capture");
    audioStatus = message.status;
    return;
  }
  if (message.type !== "finish") return;
  try {
    const module = await import("./pkg/collector/perfetto_everywhere_collector.js");
    await module.default();
    const collector = new module.WasmCollector(Number.MAX_SAFE_INTEGER);
    for (const realm of message.realms) {
      collector.register_realm(realm.id, realm.label, BigInt(realm.ticksPerSecond));
    }
    for (const metadata of message.metadata) {
      collector.register_metadata(metadata.id, metadata.namespace, metadata.label);
    }
    for (const sample of message.calibrations) {
      collector.add_calibration(
        sample.realm, sample.clock, sample.sourceTicks, sample.referenceNs, sample.uncertaintyNs,
      );
    }
    for (const [realm, batches] of ordinary) {
      let records = 0;
      for (const bytes of batches) { collector.ingest_batch(bytes); records += bytes.length / RECORD_SIZE; }
      collector.set_health(realm, BigInt(records), 0n, BigInt(batches.length), records, 0n);
    }
    for (const chunk of audioChunks) collector.ingest_batch(chunk);
    if (audioStatus) {
      collector.set_health(
        4, BigInt(audioStatus.emittedRecords), BigInt(audioStatus.droppedRecords),
        BigInt(audioStatus.chunkCount), audioStatus.highWaterRecords, 0n,
      );
    }
    const trace = collector.finish();
    self.postMessage({
      type: "trace", trace, audioRecords,
      audioCallbacks: audioStatus?.callbacks || 0,
      dropped: audioStatus?.droppedRecords || 0,
      discontinuities: audioStatus?.discontinuities || 0,
    }, [trace.buffer]);
  } catch (error) {
    self.postMessage({type: "error", error: error.stack || String(error)});
  }
};
