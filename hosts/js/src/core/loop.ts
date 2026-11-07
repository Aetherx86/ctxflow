// Drives the kernel state machine. It decides nothing itself, so the same protocol runs in
// Node, Electron and browsers; only the transport differs.

export interface Kernel {
  call(op: string, args?: Record<string, unknown>): any;
}

export interface LlmCall {
  id: string;
  model: string;
  fallbacks: string[];
  system: string;
  prompt: string;
  max_tokens: number;
  seat: number;
  kind: string;
}

export interface Reply {
  text: string;
  input: number;
  output: number;
  local: boolean;
}

export type Transport = (model: string, call: LlmCall, signal?: AbortSignal) => Promise<Reply>;

export interface Seat {
  model: string;
  title: string;
  fallbacks?: string[];
}

export interface CouncilOptions {
  question: string;
  seats: Seat[];
  chair: { model: string; fallbacks?: string[] };
  options?: string[];
  limits?: { max_tokens?: number; agree_confidence?: number; clear_share?: number; mode?: 'fast' | 'auto' | 'thorough' };
  signal?: AbortSignal;
  onEvent?: (ev: any) => void;
  onUsage?: (model: string, input: number, output: number, kind: string) => void;
}

export function estimateCouncil(kernel: Kernel, question: string, seats: number): number {
  const probe = kernel.call('council_step', {
    start: {
      question,
      seats: Array.from({ length: Math.max(seats, 2) }, () => ({ model: 'x', title: '', fallbacks: [] })),
      chair: { model: 'x', fallbacks: [] },
      options: [],
      limits: {}
    }
  });
  return probe.state.worst_case;
}

async function perform(call: LlmCall, transport: Transport, o: CouncilOptions): Promise<any> {
  const chain = [call.model, ...(call.fallbacks || [])];
  let last = '';
  for (let i = 0; i < chain.length; i++) {
    if (o.signal?.aborted) return { id: call.id, error: 'aborted' };
    try {
      const reply = await transport(chain[i], call, o.signal);
      o.onUsage?.(chain[i], reply.input, reply.output, call.kind);
      return { id: call.id, text: reply.text, model: chain[i], in: reply.input, out: reply.output };
    } catch (err: any) {
      last = String(err?.message || err);
      if (i + 1 < chain.length) {
        o.onEvent?.({ type: 'fallback', seat: call.seat, from: chain[i], to: chain[i + 1], reason: last.slice(0, 120) });
      }
    }
  }
  return { id: call.id, error: last };
}

export async function runCouncil(kernel: Kernel, transport: Transport, o: CouncilOptions): Promise<any> {
  let step = kernel.call('council_step', {
    start: {
      question: o.question,
      seats: o.seats.map((s, i) => ({ model: s.model, title: s.title, lens: i, fallbacks: s.fallbacks || [] })),
      chair: { model: o.chair.model, fallbacks: o.chair.fallbacks || [] },
      options: o.options || [],
      limits: o.limits || {}
    }
  });
  if (step.error) throw new Error(step.error);
  while (true) {
    for (const ev of step.events || []) o.onEvent?.(ev);
    if (step.done) return step.result;
    if (o.signal?.aborted) throw new Error('aborted');
    // Calls in a stage are independent, so they run at the same time. The stage takes as long as
    // its slowest model, not the sum of all of them.
    const results = await Promise.all(step.calls.map((c: LlmCall) => perform(c, transport, o)));
    step = kernel.call('council_step', { state: step.state, results });
  }
}
