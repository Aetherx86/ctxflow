import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
const entry = process.env.CTXFLOW_HOST || '../src/index.ts';
const { loadKernel, runCouncil, estimateCouncil, preflightCouncil, expandRefs, unredactText, makeTransport, makeScrubber, parseModel, isLocalModel } = await import(entry);
const kernel = await loadKernel(fs.readFileSync(new URL('../dist/kernel.wasm', import.meta.url)));

const card = (s, c = 90) => JSON.stringify({ stance: s, answer: `Use ${s}.`, confidence: c, reasons: ['r'], risks: ['x'], would_change: 'load' });
const seats = (...m) => m.map((model, i) => ({ model, title: `S${i}` }));
function fake(stances, score = () => 3, log = []) {
  return async (model, call) => {
    log.push({ model, kind: call.kind, at: Date.now() });
    let text;
    if (call.kind === 'card') text = card(stances[model]);
    else if (call.kind === 'review') {
      const scores = {};
      for (const [, l, a] of call.prompt.matchAll(/^([A-Z]): (.*)$/gm)) scores[l] = { evidence: score(a), risk: score(a), feasibility: score(a) };
      text = JSON.stringify({ scores });
    } else text = '**Decision:** chair';
    return { text, input: 100, output: 10, local: false };
  };
}

test('same kernel, same protocol: agreement stops after one stage', async () => {
  const log = [];
  const r = await runCouncil(kernel, fake({ a: 'pg', b: 'pg' }, undefined, log), { question: 'q', seats: seats('a', 'b'), chair: { model: 'z' } });
  assert.equal(r.mode, 'agreed');
  assert.equal(log.length, 2);
});

test('split answers produce a matrix; clear winner needs no chair', async () => {
  const log = [];
  const events = [];
  const r = await runCouncil(kernel, fake({ a: 'one', b: 'two', c: 'three' }, (t) => (t.includes('two') ? 5 : 2), log), {
    question: 'q', seats: seats('a', 'b', 'c'), chair: { model: 'z' }, onEvent: (e) => events.push(e)
  });
  assert.equal(r.mode, 'ranked');
  assert.match(r.final, /Use two\./);
  assert.equal(r.matrix.length, 3);
  assert.ok(!log.some((l) => l.model === 'z'));
  assert.ok(events.some((e) => e.type === 'chunk' && e.seat === 5 && e.text.includes('Evidence')));
  assert.equal(events.at(-1).type, 'done');
});

test('stage calls run in parallel', async () => {
  const t = Date.now();
  const slow = async (model, call) => { await new Promise((r) => setTimeout(r, 200)); return fake({ a: 'x', b: 'x', c: 'x', d: 'x' })(model, call); };
  await runCouncil(kernel, slow, { question: 'q', seats: seats('a', 'b', 'c', 'd'), chair: { model: 'z' } });
  assert.ok(Date.now() - t < 500, 'four 200 ms calls must not run one after another');
});

test('failure falls back and tells the UI', async () => {
  const events = [];
  const base = fake({ b: 'x', f: 'x' });
  const flaky = async (m, c) => { if (m === 'a') throw new Error('HTTP 429'); return base(m, c); };
  const r = await runCouncil(kernel, flaky, { question: 'q', seats: [{ model: 'a', title: 'A', fallbacks: ['f'] }, { model: 'b', title: 'B' }], chair: { model: 'z' }, onEvent: (e) => events.push(e) });
  assert.ok(events.some((e) => e.type === 'fallback' && e.from === 'a' && e.to === 'f'));
  assert.equal(r.mode, 'agreed');
});

test('token limit ends the run with a labeled answer', async () => {
  const r = await runCouncil(kernel, fake({ a: 'one', b: 'two', c: 'three' }), { question: 'q', seats: seats('a', 'b', 'c'), chair: { model: 'z' }, limits: { max_tokens: 400 } });
  assert.equal(r.mode, 'budget');
  assert.match(r.final, /not cross-checked/);
});

test('abort stops further calls', async () => {
  const ac = new AbortController();
  ac.abort();
  await assert.rejects(runCouncil(kernel, fake({ a: 'x', b: 'y' }), { question: 'q', seats: seats('a', 'b'), chair: { model: 'z' }, signal: ac.signal }));
});

test('worst case is known before any call', () => {
  assert.ok(estimateCouncil(kernel, 'short question', 4) > 1000);
});

test('model specs: bare Aether names and BYOK prefixes', () => {
  assert.deepEqual(parseModel('gemini-3.5-flash'), { backend: 'gemini', model: 'gemini-3.5-flash' });
  assert.deepEqual(parseModel('qwen2.5:7b'), { backend: 'ollama', model: 'qwen2.5:7b' });
  assert.deepEqual(parseModel('mlx-community/DeepSeek-R1'), { backend: 'mlx', model: 'mlx-community/DeepSeek-R1' });
  assert.deepEqual(parseModel('openrouter:google/gemini'), { backend: 'openrouter', model: 'google/gemini' });
  assert.equal(isLocalModel('qwen2.5:7b'), true);
  assert.equal(isLocalModel('gemini-3.5-flash'), false);
});

test('secrets and PII are removed before cloud calls and kept for local ones', async (t) => {
  const seen = [];
  const srv = http.createServer((req, res) => {
    let b = '';
    req.on('data', (d) => (b += d));
    req.on('end', () => {
      seen.push(JSON.parse(b).messages[1].content);
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ choices: [{ message: { content: 'ok' } }], usage: { prompt_tokens: 1, completion_tokens: 1 } }));
    });
  });
  t.after(() => srv.close());
  await new Promise((r) => srv.listen(0, r));
  const base_url = 'http://127.0.0.1:' + srv.address().port + '/v1';
  let removed = 0;
  const tr = makeTransport({
    providers: { cloud: { base_url }, near: { base_url, local: true } },
    scrub: makeScrubber(kernel, (x) => ({ text: x.replace('a@b.io', '[MAIL]'), removed: x.includes('a@b.io') ? 1 : 0 })),
    onScrub: (n) => (removed += n)
  });
  const prompt = 'key ghp_abcdefghijklmnop1234 mail a@b.io';
  await tr('cloud:m', { system: 's', prompt, max_tokens: 10 });
  await tr('near:m', { system: 's', prompt, max_tokens: 10 });
  assert.ok(!seen[0].includes('ghp_') && !seen[0].includes('a@b.io'), seen[0]);
  assert.equal(seen[1], prompt);
  assert.equal(removed, 2);
});

test('the dial reaches the kernel', async () => {
  const r = await runCouncil(kernel, fake({ a: 'one', b: 'two' }), { question: 'q', seats: seats('a', 'b'), chair: { model: 'z' }, limits: { mode: 'fast' } });
  assert.equal(r.mode, 'fast');
});

test('OpenAI-compatible transport strips thinking and reads usage', async (t) => {
  const srv = http.createServer((req, res) => {
    req.resume();
    req.on('end', () => {
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ choices: [{ message: { content: '<think>no</think>hello' } }], usage: { prompt_tokens: 7, completion_tokens: 3 } }));
    });
  });
  t.after(() => srv.close());
  await new Promise((r) => srv.listen(0, r));
  const tr = makeTransport({ ollamaHost: 'http://127.0.0.1:' + srv.address().port });
  const r = await tr('ollama:m', { system: 's', prompt: 'p', max_tokens: 10 });
  assert.equal(r.text, 'hello');
  assert.equal(r.input, 7);
  assert.equal(r.local, true);
});

test('a model name cannot become a command-line option', async () => {
  const tr = makeTransport({});
  await assert.rejects(tr('claude:--dangerously-skip-permissions', { system: 's', prompt: 'p', max_tokens: 10 }), /invalid model name/);
});

test('hidden characters in a question never reach the seats', async () => {
  const seen = [];
  const capture = async (model, call) => { seen.push(call.prompt); return { text: card('x'), input: 1, output: 1, local: false }; };
  await runCouncil(kernel, capture, { question: 'pick\u202e one\u{E0041}\u{E0042}', seats: seats('a', 'b'), chair: { model: 'z' } });
  for (const p of seen) {
    assert.ok(!/[\u202e]|[\u{E0000}-\u{E007F}]/u.test(p), JSON.stringify(p));
  }
});

test('preflightCouncil returns per-stage token estimates and independence flag', () => {
  const pre = preflightCouncil(kernel, 'Use postgres://u:pass123456@db/x?', seats('a', 'b', 'c'), { mode: 'auto' });
  assert.equal(pre.distinct_models, 3);
  assert.equal(pre.independent, true);
  assert.equal(pre.redactions, 1);
  assert.equal(pre.stage1 + pre.stage2 + pre.stage3, pre.worst_case);
  const fast = preflightCouncil(kernel, 'q', seats('a', 'a'), { mode: 'fast' });
  assert.equal(fast.independent, false);
  assert.equal(fast.stage2, 0);
  assert.equal(fast.stage3, 0);
});

test('expandRefs and unredactText round-trip through WASM kernel', () => {
  const body = '```py\n' + Array.from({ length: 20 }, (_, i) => `v${i} = ${i}`).join('\n') + '\n```';
  const orig = `top\n${body}\nmid\n${body}\nend`;
  const c = kernel.call('compress', { text: orig, code_max: 10, log_max: 6 });
  assert.equal(c.stubs.length, 1);
  const archive = Object.fromEntries(c.stubs.map((s) => [s.id, s.body]));
  const exp = expandRefs(kernel, c.text, archive);
  assert.equal(exp.text, orig);
  assert.equal(exp.missing.length, 0);

  const raw = 'API_KEY=alpha12345678 SECRET=beta9876543210 mail test@example.com';
  const red = kernel.call('redact', { text: raw });
  assert.ok(!red.text.includes('alpha12345678') && !red.text.includes('beta9876543210'));
  const unred = unredactText(kernel, red.text, red.map);
  assert.equal(unred.text, raw);
  assert.equal(unred.restored, 3);
});

test('weighted criteria and Condorcet telemetry flow through WASM council', async () => {
  const weightedTransport = async (model, call) => {
    if (call.kind === 'card') {
      return { text: card(model === 'a' ? 'risky' : model === 'b' ? 'safe' : 'weak'), input: 50, output: 20, local: false };
    }
    if (call.kind === 'review') {
      const scores = {};
      for (const [, l, a] of call.prompt.matchAll(/^([A-Z]): (.*)$/gm)) {
        scores[l] = a.includes('risky')
          ? { evidence: 5, risk: 1, feasibility: 4 }
          : a.includes('safe')
            ? { evidence: 3, risk: 5, feasibility: 4 }
            : { evidence: 2, risk: 2, feasibility: 2 };
      }
      return { text: JSON.stringify({ scores }), input: 50, output: 20, local: false };
    }
    return { text: '**Decision:** chair', input: 50, output: 20, local: false };
  };
  const r = await runCouncil(kernel, weightedTransport, {
    question: 'q',
    seats: seats('a', 'b', 'c'),
    chair: { model: 'z' },
    limits: { weights: { evidence: 1, risk: 3, feasibility: 1 } }
  });
  assert.equal(r.mode, 'ranked');
  assert.match(r.final, /Use safe\./);
  assert.equal(r.condorcet_winner.model, 'b');
  assert.equal(r.agreement, 1);
  assert.equal(r.weights.risk, 3);
});

test('expandRefs restores collapsed logs and honours only', () => {
  const lines = Array.from({ length: 120 }, (_, i) => `2026-11-21 10:00:${String(i % 60).padStart(2, '0')} INFO tick ${i}`);
  const orig = `run\n${lines.join('\n')}\n2026-11-21 10:02:00 ERROR disk full\nnext step`;
  const c = kernel.call('compress', { text: orig, code_max: 12, log_max: 6 });
  assert.equal(c.stubs.length, 1);
  assert.ok(c.text.length < orig.length / 5);
  const archive = Object.fromEntries(c.stubs.map((s) => [s.id, s.body]));
  assert.equal(expandRefs(kernel, c.text, archive).text, orig);
  const skipped = expandRefs(kernel, c.text, archive, ['000000000000']);
  assert.equal(skipped.text, c.text);
  assert.deepEqual(skipped.expanded, []);
});
