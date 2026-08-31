import "./audio-worklet-shim.js";
import { initSync, AudioChunkProducer } from "./pkg/audio/perfetto_everywhere_web.js";
import { TraceChunkProducerTransport } from "./perfetto-chunk-transport.js";

class PerfettoAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    try {
      const config = options.processorOptions;
      initSync({module: config.wasmModule});
      this.producer = new AudioChunkProducer(config.realmId, config.clockId, config.capacityRecords);
      this.transport = new TraceChunkProducerTransport(this.producer, this.port, config);
      this.flow = config.flow || 0n;
      this.record = config.record !== false;
      this.flowInstalled = false;
      this.port.onmessage = event => {
        if (event.data.type === "recycle") this.transport.recycle(event.data);
        if (event.data.type === "stop") this.transport.stop();
        if (event.data.type === "abort") this.transport.abort(event.data.reason);
      };
      this.port.postMessage({type: "ready"});
    } catch (error) {
      this.port.postMessage({type: "error", error: error.stack || String(error)});
    }
  }

  process(_inputs, outputs) {
    if (!this.producer) return false;
    const frame = currentFrame;
    if (!this.flowInstalled && this.flow !== 0n) {
      this.producer.install_flow(frame, this.flow);
      this.flowInstalled = true;
    }
    const quantum = outputs[0]?.[0]?.length || 128;
    const sequence = currentFrame / quantum;
    if (this.record) {
      this.producer.process_quantum(frame, quantum, sequence & 15, 0.2 + (sequence % 10) * 0.01);
    } else {
      this.producer.callback_only(frame, quantum);
    }
    this.transport.drain();
    return true;
  }
}

registerProcessor("perfetto-audio", PerfettoAudioProcessor);
