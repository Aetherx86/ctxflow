// Public API of the JS host: load the kernel, build a transport, run a council.
export { runCouncil, estimateCouncil } from './core/loop.ts';
export { makeTransport, parseModel, isLocalModel, ProviderError } from './adapters/providers.ts';

export async function loadKernel(bytes: BufferSource): Promise<{ call(op: string, args?: Record<string, unknown>): any }> {
  const { instance } = await WebAssembly.instantiate(bytes);
  const e: any = instance.exports;
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  return {
    call(op: string, args: Record<string, unknown> = {}) {
      e.reset();
      const data = enc.encode(JSON.stringify({ op, ...args }));
      const ptr = e.alloc(data.length);
      new Uint8Array(e.memory.buffer, ptr, data.length).set(data);
      const packed: bigint = e.call(ptr, data.length);
      const start = Number(packed >> 32n);
      const len = Number(packed & 0xffffffffn);
      return JSON.parse(dec.decode(new Uint8Array(e.memory.buffer, start, len)));
    }
  };
}

// Removes credentials with the kernel, then anything else the caller's scanner covers.
export function makeScrubber(kernel: { call(op: string, args?: Record<string, unknown>): any }, extra?: (t: string) => { text: string; removed: number }) {
  return (text: string) => {
    const a = kernel.call('redact', { text });
    const b = extra ? extra(a.text) : { text: a.text, removed: 0 };
    return { text: b.text, removed: a.map.length + b.removed };
  };
}
