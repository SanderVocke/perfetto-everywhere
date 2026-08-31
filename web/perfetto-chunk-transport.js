export const TRACE_CHUNK_PROTOCOL_VERSION = 2;
export const TRACE_RECORD_SIZE = 48;

export class TraceChunkProducerTransport {
  constructor(producer, port, {
    captureId, capacityRecords, chunkBytes = capacityRecords * TRACE_RECORD_SIZE, poolSize = 3,
  }) {
    if (!Number.isSafeInteger(captureId) || captureId <= 0) throw new Error("invalid capture ID");
    if (!Number.isInteger(poolSize) || poolSize < 2) throw new Error("chunk pool is too small");
    if (!Number.isInteger(chunkBytes)
        || chunkBytes < capacityRecords * TRACE_RECORD_SIZE
        || chunkBytes % TRACE_RECORD_SIZE !== 0) {
      throw new Error("chunk cannot hold the largest accepted record group");
    }
    this.producer = producer;
    this.port = port;
    this.captureId = captureId;
    this.chunkBytes = chunkBytes;
    this.available = [];
    this.inFlight = new Array(poolSize).fill(false);
    this.inFlightCount = 0;
    this.maxInFlight = 0;
    for (let token = 0; token < poolSize; token++) {
      const buffer = new ArrayBuffer(chunkBytes);
      this.available.push({token, buffer, bytes: new Uint8Array(buffer), used: 0});
    }
    this.active = this.available.pop();
    this.sequence = 0;
    this.stopping = null;
    this.finished = false;
  }

  drain() {
    while (this.producer.available_records() > 0) {
      if (!this.active) this.active = this.available.pop() || null;
      if (!this.active) return false;
      const drained = this.producer.drain(this.chunkBytes - this.active.used);
      if (drained.length > 0) {
        this.active.bytes.set(drained, this.active.used);
        this.active.used += drained.length;
      }
      if (this.active.used === this.chunkBytes
          || (drained.length === 0 && this.producer.available_records() > 0)) {
        this.transferActive();
      }
    }
    return true;
  }

  transferActive() {
    if (!this.active || this.active.used === 0) return;
    const chunk = this.active;
    this.active = this.available.pop() || null;
    this.inFlight[chunk.token] = true;
    this.inFlightCount++;
    this.maxInFlight = Math.max(this.maxInFlight, this.inFlightCount);
    this.port.postMessage({
      type: "trace-chunk", captureId: this.captureId, sequence: this.sequence,
      poolToken: chunk.token, usedBytes: chunk.used, buffer: chunk.buffer,
    }, [chunk.buffer]);
    this.sequence++;
  }

  recycle(message) {
    if (message.captureId !== this.captureId || this.finished) return;
    if (!this.inFlight[message.poolToken]) throw new Error("invalid recycled pool token");
    if (!(message.buffer instanceof ArrayBuffer) || message.buffer.byteLength !== this.chunkBytes) {
      throw new Error("invalid recycled buffer");
    }
    this.inFlight[message.poolToken] = false;
    this.inFlightCount--;
    this.available.push({
      token: message.poolToken, buffer: message.buffer,
      bytes: new Uint8Array(message.buffer), used: 0,
    });
    if (!this.active) this.active = this.available.pop();
    if (this.stopping) this.tryFinish();
  }

  stop(status = {}) {
    if (this.stopping || this.finished) return;
    this.producer.finish();
    this.stopping = status;
    this.tryFinish();
  }

  abort(reason = "capture aborted") {
    if (this.finished) return;
    this.producer.finish();
    this.finished = true;
    this.port.postMessage({type: "trace-aborted", captureId: this.captureId, reason});
  }

  tryFinish() {
    if (!this.stopping || this.finished || !this.drain()) return;
    this.transferActive();
    this.finished = true;
    this.port.postMessage({
      type: "trace-stopped", captureId: this.captureId, chunkCount: this.sequence,
      emittedRecords: this.producer.emitted_records(),
      droppedRecords: this.producer.dropped_records(),
      highWaterRecords: this.producer.high_water_records(),
      callbacks: this.producer.callbacks(),
      discontinuities: this.producer.discontinuities(),
      maxInFlight: this.maxInFlight,
      ...this.stopping,
    });
  }
}
