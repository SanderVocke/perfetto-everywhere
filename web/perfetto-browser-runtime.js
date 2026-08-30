export const PERFETTO_RECORD_SIZE = 48;
export const PERFETTO_AUDIO_HEADER_BYTES = 64;
export const PERFETTO_AUDIO_MAGIC = 0x50454631;

export function createAudioRing(capacity, sampleRate, quantumFrames) {
  const sab = new SharedArrayBuffer(PERFETTO_AUDIO_HEADER_BYTES + capacity * PERFETTO_RECORD_SIZE);
  const header = new Int32Array(sab, 0, 16);
  header[0] = PERFETTO_AUDIO_MAGIC;
  header[1] = capacity;
  header[9] = sampleRate;
  header[10] = quantumFrames;
  return {sab, header, capacity};
}

export function metadataId(namespace, label) {
  let hash = (0x811c9dc5 ^ namespace) >>> 0;
  for (const byte of new TextEncoder().encode(label)) {
    hash = Math.imul((hash ^ byte) >>> 0, 0x01000193) >>> 0;
  }
  return hash || 1;
}

export function metadataEntries(definitions) {
  return definitions.map(([namespace, label]) => ({
    id: metadataId(namespace, label), namespace, label,
  }));
}

export function waitMessage(endpoint, predicate) {
  return new Promise(resolve => {
    const listener = event => {
      if (!predicate(event.data)) return;
      endpoint.removeEventListener("message", listener);
      resolve(event.data);
    };
    endpoint.addEventListener("message", listener);
  });
}

export function audioCalibration(context, sampleRate) {
  if (typeof context.getOutputTimestamp === "function") {
    const sample = context.getOutputTimestamp();
    return {
      sourceTicks: BigInt(Math.round(sample.contextTime * sampleRate)),
      referenceNs: BigInt(Math.round((performance.timeOrigin + sample.performanceTime) * 1e6)),
      uncertaintyNs: 100_000n,
      contextTime: sample.contextTime,
    };
  }
  const before = performance.timeOrigin + performance.now();
  const contextTime = context.currentTime;
  const after = performance.timeOrigin + performance.now();
  return {
    sourceTicks: BigInt(Math.round(contextTime * sampleRate)),
    referenceNs: BigInt(Math.round(((before + after) / 2) * 1e6)),
    uncertaintyNs: BigInt(Math.max(1, Math.round(((after - before) / 2) * 1e6))),
    contextTime,
  };
}

export function ordinaryCalibration(realm, clock, localMs, referenceMs, uncertaintyMs) {
  return {
    realm,
    clock,
    sourceTicks: BigInt(Math.round(localMs * 1e6)),
    referenceNs: BigInt(Math.round(referenceMs * 1e6)),
    uncertaintyNs: BigInt(Math.max(1, Math.round(uncertaintyMs * 1e6))),
  };
}

export class BrowserCaptureController {
  constructor({audioCapacity = 8192, sampleRate = 48000, quantumFrames = 128} = {}) {
    if (!crossOriginIsolated || typeof SharedArrayBuffer !== "function") {
      throw new Error("perfetto-everywhere browser capture requires COOP/COEP and SharedArrayBuffer");
    }
    this.audioRing = createAudioRing(audioCapacity, sampleRate, quantumFrames);
    this.sampleRate = sampleRate;
    this.realms = [];
    this.metadata = new Map();
    this.calibrations = [];
    this.collector = new Worker("multirealm-collector-worker.js", {type: "module"});
    this.started = false;
    this.finished = false;
    this.drainTimer = null;
  }

  async start() {
    if (this.started) throw new Error("browser capture already started");
    this.started = true;
    const ready = waitMessage(this.collector, data => data.type === "ready");
    this.collector.postMessage({type: "start-audio", sab: this.audioRing.sab});
    await ready;
  }

  registerRealm(id, label, ticksPerSecond) {
    if (this.realms.some(realm => realm.id === id)) throw new Error(`duplicate realm ${id}`);
    this.realms.push({id, label, ticksPerSecond});
  }

  registerMetadata(entries) {
    for (const entry of entries) {
      const existing = this.metadata.get(entry.id);
      if (existing && (existing.namespace !== entry.namespace || existing.label !== entry.label)) {
        throw new Error(`metadata collision ${entry.id}`);
      }
      this.metadata.set(entry.id, entry);
    }
  }

  addCalibration(sample) {
    this.calibrations.push(sample);
  }

  submitRecords(realm, records) {
    if (!this.started || this.finished) throw new Error("capture is not accepting records");
    this.collector.postMessage({type: "records", realm, records}, [records.buffer]);
  }

  startAudioDrain(intervalMs = 2) {
    if (this.drainTimer !== null) return;
    this.drainTimer = setInterval(() => this.collector.postMessage({type: "drain"}), intervalMs);
  }

  abort() {
    if (this.drainTimer !== null) clearInterval(this.drainTimer);
    this.finished = true;
    this.collector.terminate();
  }

  async finish() {
    if (!this.started || this.finished) throw new Error("capture cannot be finished in this state");
    this.finished = true;
    if (this.drainTimer !== null) clearInterval(this.drainTimer);
    const pending = waitMessage(this.collector, data => data.type === "trace" || data.type === "error");
    this.collector.postMessage({
      type: "finish",
      realms: this.realms,
      metadata: [...this.metadata.values()],
      calibrations: this.calibrations,
    });
    const result = await pending;
    this.collector.terminate();
    if (result.type === "error") throw new Error(result.error);
    return result;
  }
}

export function traceDownload(bytes, filename = "capture.pftrace") {
  const blob = new Blob([bytes], {type: "application/octet-stream"});
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.textContent = `Download ${filename}`;
  anchor.addEventListener("click", () => setTimeout(() => URL.revokeObjectURL(url), 1000), {once: true});
  return {blob, anchor};
}
