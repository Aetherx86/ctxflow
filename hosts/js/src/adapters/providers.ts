// Provider I/O for browsers and Electron. No SDK is required.

export class ProviderError extends Error {}

export interface ProviderConfig {
  env?: Record<string, string | undefined>;
  ollamaHost?: string;
  mlxPorts?: number[];
  providers?: Record<string, { base_url: string; api_key_env?: string; local?: boolean }>;
  claudeBinary?: string;
  scrub?: (text: string) => { text: string; removed: number };
  onScrub?: (removed: number) => void;
}

const HOSTED: Record<string, { base_url: string; api_key_env: string }> = {
  openai: { base_url: 'https://api.openai.com/v1', api_key_env: 'OPENAI_API_KEY' },
  openrouter: { base_url: 'https://openrouter.ai/api/v1', api_key_env: 'OPENROUTER_API_KEY' },
  groq: { base_url: 'https://api.groq.com/openai/v1', api_key_env: 'GROQ_API_KEY' },
  deepseek: { base_url: 'https://api.deepseek.com/v1', api_key_env: 'DEEPSEEK_API_KEY' }
};

export function parseModel(spec: string, cfg: ProviderConfig = {}): { backend: string; model: string } {
  const i = spec.indexOf(':');
  const head = i > 0 ? spec.slice(0, i) : '';
  if (['gemini', 'anthropic', 'claude', 'ollama', 'mlx', 'lmstudio', ...Object.keys(HOSTED), ...Object.keys(cfg.providers || {})].includes(head)) {
    return { backend: head, model: spec.slice(i + 1) };
  }
  // Bare names, as saved by older Aether settings.
  if (spec.startsWith('gemini')) return { backend: 'gemini', model: spec };
  if (spec.includes('/')) return { backend: 'mlx', model: spec };
  return { backend: 'ollama', model: spec };
}

export function isLocalModel(spec: string, cfg: ProviderConfig = {}): boolean {
  const { backend } = parseModel(spec, cfg);
  return backend === 'ollama' || backend === 'mlx' || backend === 'lmstudio' || !!cfg.providers?.[backend]?.local;
}

function stripThink(text: string): string {
  return text.replace(/<think>[\s\S]*?<\/think>\s*/g, '');
}

async function post(url: string, headers: Record<string, string>, body: unknown, label: string, signal?: AbortSignal): Promise<any> {
  let res: Response;
  try {
    res = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body), signal });
  } catch (err: any) {
    throw new ProviderError(label + ' unavailable: ' + (err?.message || err));
  }
  if (!res.ok) {
    const detail = (await res.text()).slice(0, 300);
    throw new ProviderError(label + ' HTTP ' + res.status + ': ' + detail);
  }
  return res.json();
}

async function gemini(model: string, c: any, cfg: ProviderConfig, signal?: AbortSignal) {
  const key = cfg.env?.GEMINI_API_KEY;
  if (!key) throw new ProviderError('GEMINI_API_KEY is not set');
  const url = 'https://generativelanguage.googleapis.com/v1beta/models/' + encodeURIComponent(model) + ':generateContent';
  const body = {
    systemInstruction: { parts: [{ text: c.system }] },
    contents: [{ role: 'user', parts: [{ text: c.prompt }] }],
    // Thinking counts against this limit, so leave room or the JSON gets cut off.
    generationConfig: { maxOutputTokens: Math.max(c.max_tokens * 3, 1024) }
  };
  const data = await post(url, { 'x-goog-api-key': key }, body, 'gemini', signal);
  const text = (data.candidates?.[0]?.content?.parts || []).map((p: any) => p.text || '').join('');
  if (!text) throw new ProviderError('gemini returned no text (' + (data.candidates?.[0]?.finishReason || data.promptFeedback?.blockReason || 'unknown') + ')');
  const u = data.usageMetadata || {};
  return { text, input: u.promptTokenCount || 0, output: (u.candidatesTokenCount || 0) + (u.thoughtsTokenCount || 0) };
}

async function compat(label: string, base: string, key: string, model: string, c: any, local: boolean, signal?: AbortSignal) {
  const headers: Record<string, string> = key ? { Authorization: 'Bearer ' + key } : {};
  const data = await post(base.replace(/\/$/, '') + '/chat/completions', headers, {
    model,
    messages: [{ role: 'system', content: c.system }, { role: 'user', content: c.prompt }],
    max_tokens: local ? Math.max(c.max_tokens * 3, 1024) : c.max_tokens
  }, label, signal);
  const u = data.usage || {};
  return { text: data.choices?.[0]?.message?.content || '', input: u.prompt_tokens || 0, output: u.completion_tokens || 0 };
}

async function ollamaAuto(host: string): Promise<string> {
  let names: string[] = [];
  try {
    const res = await fetch(host + '/api/tags');
    names = ((await res.json()).models || []).map((m: any) => m.name);
  } catch (err: any) {
    throw new ProviderError('ollama unavailable: ' + (err?.message || err));
  }
  if (!names.length) throw new ProviderError('ollama has no models installed');
  return names[0];
}

async function claudeCli(model: string, c: any, cfg: ProviderConfig, signal?: AbortSignal) {
  let spawn: any;
  try {
    spawn = (await import('node:child_process')).spawn;
  } catch {
    throw new ProviderError('claude: models need ctxflow serve when running in a browser');
  }
  const args = ['-p', '--output-format', 'json', '--system-prompt', c.system, '--tools', '', '--no-session-persistence', '--disable-slash-commands', '--strict-mcp-config'];
  if (model && model !== 'default') args.push('--model', model);
  return new Promise<any>((resolve, reject) => {
    const proc = spawn(cfg.claudeBinary || 'claude', args, { stdio: ['pipe', 'pipe', 'pipe'], signal });
    let out = '';
    let err = '';
    proc.stdout.on('data', (d: any) => (out += d));
    proc.stderr.on('data', (d: any) => (err += d));
    proc.on('error', (e: any) => reject(new ProviderError('claude unavailable: ' + e.message)));
    proc.on('close', () => {
      let data: any = {};
      try {
        data = JSON.parse(out.trim().split('\n').pop() || '{}');
      } catch {}
      if (!data.result || data.is_error) return reject(new ProviderError('claude failed: ' + String(data.result || err || out).slice(0, 300)));
      const u = data.usage || {};
      resolve({
        text: data.result,
        input: (u.input_tokens || 0) + (u.cache_creation_input_tokens || 0) + (u.cache_read_input_tokens || 0),
        output: u.output_tokens || 0
      });
    });
    proc.stdin.end(c.prompt);
  });
}

export function makeTransport(cfg: ProviderConfig = {}) {
  return async (spec: string, call: any, signal?: AbortSignal) => {
    const { backend, model } = parseModel(spec, cfg);
    const local = isLocalModel(spec, cfg);
    let c = call;
    if (!local && cfg.scrub) {
      const s = cfg.scrub(call.prompt);
      if (s.removed) cfg.onScrub?.(s.removed);
      c = { ...call, prompt: s.text };
    }
    let r: { text: string; input: number; output: number };
    if (backend === 'gemini') {
      r = await gemini(model, c, cfg, signal);
    } else if (backend === 'anthropic') {
      throw new ProviderError('anthropic: models need ctxflow serve, which keeps the key out of the browser');
    } else if (backend === 'claude') {
      r = await claudeCli(model, c, cfg, signal);
    } else if (backend === 'ollama') {
      const host = cfg.ollamaHost || 'http://127.0.0.1:11434';
      r = await compat('ollama', host + '/v1', '', model === 'auto' ? await ollamaAuto(host) : model, c, true, signal);
    } else if (backend === 'mlx') {
      let last: any;
      r = null as any;
      for (const port of cfg.mlxPorts || [8080, 8088]) {
        try {
          r = await compat('mlx', 'http://127.0.0.1:' + port + '/v1', '', model, c, true, signal);
          break;
        } catch (e) {
          last = e;
        }
      }
      if (!r) throw last;
    } else if (backend === 'lmstudio') {
      r = await compat('lmstudio', 'http://127.0.0.1:1234/v1', '', model, c, true, signal);
    } else {
      const p: any = cfg.providers?.[backend] || HOSTED[backend];
      if (!p) throw new ProviderError('unknown provider ' + backend);
      const key = p.api_key_env ? cfg.env?.[p.api_key_env] || '' : '';
      if (p.api_key_env && !key) throw new ProviderError(p.api_key_env + ' is not set');
      r = await compat(backend, p.base_url, key, model, c, !!p.local, signal);
    }
    return { ...r, text: stripThink(r.text), local };
  };
}
