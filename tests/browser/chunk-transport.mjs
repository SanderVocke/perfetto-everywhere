import assert from "node:assert/strict";
import test from "node:test";
import { TraceChunkProducerTransport } from "../../web/perfetto-chunk-transport.js";

class Producer {
  constructor(records) { this.records = records; this.enabled = true; }
  available_records() { return this.records; }
  drain(maximum) {
    if (this.records === 0 || maximum < 48) return new Uint8Array();
    this.records--;
    return new Uint8Array(48);
  }
  finish() { this.enabled = false; }
  emitted_records() { return 4n; }
  dropped_records() { return 0n; }
  high_water_records() { return 4; }
  callbacks() { return 1n; }
  discontinuities() { return 0n; }
}

class BulkProducer extends Producer {
  drain(maximum) {
    const count = Math.min(this.records, Math.floor(maximum / 48));
    this.records -= count;
    return new Uint8Array(count * 48);
  }
}

class Port {
  constructor() { this.messages = []; }
  postMessage(message, transfer = []) { this.messages.push(structuredClone(message, {transfer})); }
}

const config = {captureId: 5, capacityRecords: 1, chunkBytes: 48, poolSize: 3};
function recycle(transport, chunk) {
  const buffer = structuredClone(chunk.buffer, {transfer: [chunk.buffer]});
  transport.recycle({captureId: 5, poolToken: chunk.poolToken, buffer});
}

test("captures beyond the complete pool by recycling", () => {
  const producer = new Producer(10);
  const port = new Port();
  const transport = new TraceChunkProducerTransport(producer, port, config);
  while (producer.records > 0) {
    transport.drain();
    const chunk = port.messages.find(message => message.type === "trace-chunk");
    if (chunk) { port.messages.splice(port.messages.indexOf(chunk), 1); recycle(transport, chunk); }
  }
  transport.stop();
  assert.equal(port.messages.find(message => message.type === "trace-stopped").chunkCount, 10);
});

test("capture length exceeds the former 262144-record retention limit", () => {
  const producer = new BulkProducer(262145);
  const port = new Port();
  const transport = new TraceChunkProducerTransport(producer, port, {
    captureId: 5, capacityRecords: 8192, chunkBytes: 8192 * 48, poolSize: 3,
  });
  while (producer.records > 0) {
    transport.drain();
    const chunk = port.messages.find(message => message.type === "trace-chunk");
    if (chunk) { port.messages.splice(port.messages.indexOf(chunk), 1); recycle(transport, chunk); }
  }
  transport.stop();
  assert.equal(port.messages.find(message => message.type === "trace-stopped").chunkCount, 33);
});

test("stop during starvation completes asynchronously after recycle", () => {
  const producer = new Producer(4);
  const port = new Port();
  const transport = new TraceChunkProducerTransport(producer, port, config);
  transport.drain();
  transport.stop();
  assert.equal(transport.finished, false);
  recycle(transport, port.messages.find(message => message.type === "trace-chunk"));
  assert.equal(transport.finished, true);
});

test("empty capture declares zero chunks and invalid capacity is rejected", () => {
  const producer = new Producer(0);
  const port = new Port();
  const transport = new TraceChunkProducerTransport(producer, port, config);
  transport.stop();
  assert.equal(port.messages.find(message => message.type === "trace-stopped").chunkCount, 0);
  assert.throws(() => new TraceChunkProducerTransport(producer, port, {...config, chunkBytes: 47}));
});

test("abort is terminal and explicit", () => {
  const producer = new Producer(1);
  const port = new Port();
  const transport = new TraceChunkProducerTransport(producer, port, config);
  transport.abort("timeout");
  assert.equal(transport.finished, true);
  assert.equal(port.messages.at(-1).type, "trace-aborted");
  assert.equal(port.messages.at(-1).reason, "timeout");
});
