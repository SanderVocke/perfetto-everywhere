const RECORD_SIZE = 48;
let chunks = [];
let recordCount = 0;
let nextSequence = 0;

self.onmessage = async event => {
  const message = event.data;
  if (message.type === "start") {
    chunks = [];
    recordCount = 0;
    nextSequence = 0;
    self.postMessage({type: "ready"});
    return;
  }
  if (message.type === "trace-chunk") {
    if (message.sequence !== nextSequence) throw new Error("non-contiguous audio trace chunk");
    if (message.usedBytes <= 0 || message.usedBytes % RECORD_SIZE !== 0
        || message.usedBytes > message.buffer.byteLength) {
      throw new Error("invalid audio trace chunk length");
    }
    chunks.push(new Uint8Array(message.buffer, 0, message.usedBytes).slice());
    recordCount += message.usedBytes / RECORD_SIZE;
    nextSequence++;
    self.postMessage({
      type: "recycle", captureId: message.captureId, poolToken: message.poolToken,
      buffer: message.buffer,
    }, [message.buffer]);
    return;
  }
  if (message.type !== "finish") return;
  try {
    if (message.status.chunkCount !== nextSequence) throw new Error("incomplete audio trace");
    const module = await import("./pkg/collector/perfetto_everywhere_collector.js");
    await module.default();
    const collector = new module.WasmCollector(Number.MAX_SAFE_INTEGER);
    collector.register_realm(4, "AudioWorklet", BigInt(message.sampleRate));
    for (const metadata of message.metadata) {
      collector.register_metadata(metadata.id, metadata.namespace, metadata.label);
    }
    for (const sample of message.calibrations) {
      collector.add_calibration(4, 104, sample.sourceFrame, sample.referenceNs, sample.uncertaintyNs);
    }
    for (const chunk of chunks) collector.ingest_batch(chunk);
    collector.set_health(
      4, BigInt(message.status.emittedRecords), BigInt(message.status.droppedRecords),
      BigInt(message.status.chunkCount), message.status.highWaterRecords, 0n,
    );
    const trace = collector.finish();
    self.postMessage({
      type: "trace", trace, records: recordCount, callbacks: message.status.callbacks,
      dropped: message.status.droppedRecords,
      discontinuities: message.status.discontinuities,
      highWater: message.status.highWaterRecords,
      drains: message.status.chunkCount,
    }, [trace.buffer]);
  } catch (error) {
    self.postMessage({type: "error", error: error.stack || String(error)});
  }
};
