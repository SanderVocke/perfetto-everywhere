// AudioWorkletGlobalScope omits TextDecoder in Chromium. wasm-bindgen creates
// one at glue-module evaluation time even though the steady-state producer does
// not decode strings. This allocation-free ASCII fallback is used only for
// initialization/error text.
if (typeof globalThis.TextDecoder === "undefined") {
  globalThis.TextDecoder = class {
    decode(bytes = new Uint8Array()) {
      let output = "";
      for (let index = 0; index < bytes.length; index++) {
        output += String.fromCharCode(bytes[index]);
      }
      return output;
    }
  };
}
