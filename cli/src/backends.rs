//! LLM backends. Every call is stateless: one self-contained prompt in, one answer out. Memory is
//! assembled by ctxflow, not the provider, which is what makes switching models mid-task free.
use crate::http;
use crate::util::{find_in_path, k, run_process, strs};
use ctxflow_kernel::compress::{estimate, sanitize};
use ctxflow_kernel::json::{obj, parse, st, V};
use std::path::Path;
use std::time::Duration;

#[derive(Default)]
pub struct Reply {
    pub text: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub estimated: bool,
    pub cost_usd: Option<f64>,
    pub redacted: u64,
}

pub struct BackendError {
    pub msg: String,
    pub rate_limited: bool,
}

impl BackendError {
    fn new(msg: impl Into<String>) -> Self {
        BackendError { msg: msg.into(), rate_limited: false }
    }

    fn limited(msg: impl Into<String>) -> Self {
        BackendError { msg: msg.into(), rate_limited: true }
    }
}

type Out = Result<Reply, BackendError>;

pub fn parse_spec(spec: &str) -> Result<(String, String), BackendError> {
    match spec.split_once(':') {
        Some((b, m)) => Ok((b.trim().to_ascii_lowercase(), m.trim().to_string())),
        None => Err(BackendError::new(format!("model spec must look like 'backend:model', got {spec:?}"))),
    }
}

pub fn is_local(backend: &str, cfg: &V) -> bool {
    backend == "ollama" || cfg.get("providers").get(backend).get("local").b()
}

pub fn call(spec: &str, system: &str, prompt: &str, cfg: &V, max_tokens: Option<u64>) -> Out {
    let (backend, model) = parse_spec(spec)?;
    // A model name becomes a command-line argument for some backends.
    if model.starts_with('-') || model.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(BackendError::new(format!("invalid model name {model:?}")));
    }
    // Hidden characters can carry instructions a person reviewing the input never sees.
    let (clean_system, _) = sanitize(system);
    let (clean_prompt, _) = sanitize(prompt);
    let system = clean_system.as_str();
    let mut redacted = 0;
    let scrubbed;
    let mut text = clean_prompt.as_str();
    if !is_local(&backend, cfg) {
        let r = k("redact", vec![("text", st(text))]);
        redacted = r.get("map").arr().len() as u64;
        scrubbed = r.get("text").s().to_string();
        text = &scrubbed;
    }
    let mut res = match backend.as_str() {
        "gemini" => gemini(&model, system, text, cfg, max_tokens),
        "claude" => claude(&model, system, text, cfg),
        "ollama" => ollama(&model, system, text, cfg, max_tokens),
        "anthropic" => anthropic(&model, system, text, cfg, max_tokens),
        "echo" => Ok(echo(&model, system, text)),
        other if matches!(cfg.get("providers").get(other), V::Obj(_)) => openai_compat(other, &model, system, text, cfg, max_tokens),
        other => Err(BackendError::new(format!("unknown backend {other:?}"))),
    }?;
    res.text = sanitize(&strip_think(&res.text)).0;
    res.redacted = redacted;
    Ok(res)
}

/// Reasoning models emit <think> blocks that are not part of the answer.
fn strip_think(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(a) = rest.find("<think>") {
        let Some(len) = rest[a..].find("</think>") else { break };
        out.push_str(&rest[..a]);
        rest = rest[a + len + "</think>".len()..].trim_start();
    }
    out.push_str(rest);
    out
}

fn snippet(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn timeout_of(section: &V, default: u64) -> u64 {
    section.get("timeout_sec").n(default as f64) as u64
}

// ── Gemini (Interactions API, store=false, so nothing stays on the server) ───────

fn gemini_text(data: &V) -> Result<String, BackendError> {
    if let V::Str(s) = data.get("output_text") {
        return Ok(s.clone());
    }
    let mut chunks = String::new();
    for step in data.get("steps").arr() {
        if step.get("type").s() == "model_output" {
            for c in step.get("content").arr() {
                if c.get("type").s() == "text" {
                    chunks.push_str(c.get("text").s());
                }
            }
        }
    }
    if chunks.is_empty() {
        return Err(BackendError::new(format!("gemini returned no text (status={})", data.get("status").s())));
    }
    Ok(chunks)
}

fn gemini(model: &str, system: &str, prompt: &str, cfg: &V, max_tokens: Option<u64>) -> Out {
    let g = cfg.get("gemini");
    let env = g.get("api_key_env").s();
    let key = std::env::var(env).unwrap_or_default();
    if key.is_empty() {
        return Err(BackendError::new(format!("${env} is not set")));
    }
    let mut generation = Vec::new();
    if !g.get("thinking_level").s().is_empty() {
        generation.push(("thinking_level", g.get("thinking_level").clone()));
    }
    if let Some(m) = max_tokens {
        // Thinking counts against this limit, so leave room or the JSON gets cut off.
        generation.push(("max_output_tokens", V::Num(m.saturating_mul(3).max(1024) as f64)));
    }
    let mut body = vec![("model", st(model)), ("input", st(prompt)), ("store", V::Bool(false)), ("system_instruction", st(system))];
    if !generation.is_empty() {
        body.push(("generation_config", obj(generation)));
    }
    let body = obj(body).dump();
    let headers = [("x-goog-api-key", key), ("Api-Revision", g.get("api_revision").s().to_string())];
    let retries = g.get("max_retries").n(2.0) as u64;
    let timeout = timeout_of(g, 180);
    let mut attempt = 0;
    let data = loop {
        let wait = match http::post_json(g.get("endpoint").s(), &headers, &body, timeout) {
            Ok(r) if (200..300).contains(&r.status) => break parse(&r.body).unwrap_or(V::Null),
            Ok(r) if r.status == 429 => {
                // A 429 usually means a per-minute limit: wait a little, then let the router move on.
                if attempt >= retries {
                    return Err(BackendError::limited(format!("gemini 429 rate limited: {}", snippet(&r.body, 500))));
                }
                (5 * (attempt + 1)).min(30)
            }
            Ok(r) if r.status >= 500 && attempt < retries => 2 * (attempt + 1),
            Ok(r) => return Err(BackendError::new(format!("gemini HTTP {}: {}", r.status, snippet(&r.body, 500)))),
            Err(e) if attempt < retries => {
                let _ = e;
                2 * (attempt + 1)
            }
            Err(e) => return Err(BackendError::new(format!("gemini network error: {e}"))),
        };
        attempt += 1;
        std::thread::sleep(Duration::from_secs(wait));
    };
    let text = gemini_text(&data)?;
    let usage = data.get("usage");
    let input = usage.get("total_input_tokens").n(0.0) as u64;
    let output = (usage.get("total_output_tokens").n(0.0) + usage.get("total_thought_tokens").n(0.0)) as u64;
    Ok(Reply {
        text,
        model: format!("gemini:{model}"),
        input_tokens: input,
        output_tokens: output,
        total_tokens: usage.get("total_tokens").n((input + output) as f64) as u64,
        estimated: !matches!(usage, V::Obj(_)),
        ..Reply::default()
    })
}

// ── Claude Code CLI (headless `claude -p`) ───────────────────────────────────────

fn claude(model: &str, system: &str, prompt: &str, cfg: &V) -> Out {
    let c = cfg.get("claude");
    let name = c.get("binary").s();
    let name = if name.is_empty() { "claude" } else { name };
    let Some(bin) = find_in_path(name) else {
        return Err(BackendError::new("claude CLI not found on PATH"));
    };
    let mut args: Vec<String> = ["-p", "--output-format", "json", "--system-prompt", system].iter().map(|s| s.to_string()).collect();
    if !model.is_empty() && model != "default" {
        args.push("--model".into());
        args.push(model.into());
    }
    args.extend(strs(c.get("extra_args")));
    let cwd = c.get("cwd").s();
    let dir = if cwd.is_empty() { None } else { Some(Path::new(cwd)) };
    let proc = run_process(&bin, &args, prompt, dir, Duration::from_secs(timeout_of(c, 900))).map_err(BackendError::new)?;
    let raw = proc.stdout.trim();
    let data = parse(raw.lines().next_back().unwrap_or("{}")).unwrap_or(V::Null);
    if !proc.ok || data.get("is_error").b() {
        let msg = [data.get("result").s(), proc.stderr.trim(), raw].into_iter().find(|s| !s.is_empty()).map_or_else(|| format!("exit {:?}", proc.code), str::to_string);
        let low = msg.to_lowercase();
        let limited = ["usage limit", "rate limit", "limit reached", "429"].iter().any(|w| low.contains(w));
        let text = format!("claude failed: {}", snippet(&msg, 500));
        return Err(if limited { BackendError::limited(text) } else { BackendError::new(text) });
    }
    let usage = data.get("usage");
    let input = (usage.get("input_tokens").n(0.0) + usage.get("cache_creation_input_tokens").n(0.0) + usage.get("cache_read_input_tokens").n(0.0)) as u64;
    let output = usage.get("output_tokens").n(0.0) as u64;
    Ok(Reply {
        text: data.get("result").s().to_string(),
        model: format!("claude:{model}"),
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + output,
        estimated: !matches!(usage, V::Obj(_)),
        cost_usd: if let V::Num(c) = data.get("total_cost_usd") { Some(*c) } else { None },
        ..Reply::default()
    })
}

// ── Ollama ───────────────────────────────────────────────────────────────────────

pub fn ollama_auto(host: &str) -> Result<String, BackendError> {
    let r = http::get(&format!("{}/api/tags", host.trim_end_matches('/')), 3).map_err(|e| BackendError::new(format!("ollama unavailable: {e}")))?;
    let data = parse(&r.body).unwrap_or(V::Null);
    match data.get("models").arr().first() {
        Some(m) => Ok(m.get("name").s().to_string()),
        None => Err(BackendError::new("ollama has no models installed")),
    }
}

fn ollama(model: &str, system: &str, prompt: &str, cfg: &V, max_tokens: Option<u64>) -> Out {
    let o = cfg.get("ollama");
    let host = o.get("host").s().trim_end_matches('/');
    let resolved = if model == "auto" { ollama_auto(host)? } else { model.to_string() };
    let mut body = vec![
        ("model", st(&resolved)),
        ("stream", V::Bool(false)),
        ("messages", V::Arr(vec![
            obj(vec![("role", st("system")), ("content", st(system))]),
            obj(vec![("role", st("user")), ("content", st(prompt))]),
        ])),
    ];
    if let Some(m) = max_tokens {
        body.push(("options", obj(vec![("num_predict", V::Num(m.saturating_mul(3).max(1024) as f64))])));
    }
    let r = http::post_json(&format!("{host}/api/chat"), &[], &obj(body).dump(), timeout_of(o, 600)).map_err(|e| BackendError::new(format!("ollama unavailable: {e}")))?;
    if !(200..300).contains(&r.status) {
        return Err(BackendError::new(format!("ollama HTTP {}: {}", r.status, snippet(&r.body, 300))));
    }
    let data = parse(&r.body).unwrap_or(V::Null);
    let (input, output) = (data.get("prompt_eval_count").n(0.0) as u64, data.get("eval_count").n(0.0) as u64);
    Ok(Reply {
        text: data.get("message").get("content").s().to_string(),
        model: format!("ollama:{resolved}"),
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + output,
        ..Reply::default()
    })
}

// ── Any OpenAI-compatible service, hosted or local (bring your own key) ──────────

fn openai_compat(name: &str, model: &str, system: &str, prompt: &str, cfg: &V, max_tokens: Option<u64>) -> Out {
    let p = cfg.get("providers").get(name);
    let env = p.get("api_key_env").s();
    let key = if env.is_empty() { String::new() } else { std::env::var(env).unwrap_or_default() };
    if !env.is_empty() && key.is_empty() {
        return Err(BackendError::new(format!("${env} is not set")));
    }
    let mut body = vec![
        ("model", st(model)),
        ("messages", V::Arr(vec![
            obj(vec![("role", st("system")), ("content", st(system))]),
            obj(vec![("role", st("user")), ("content", st(prompt))]),
        ])),
    ];
    if let Some(m) = max_tokens {
        let limit = if p.get("local").b() { m.saturating_mul(3).max(1024) } else { m };
        body.push(("max_tokens", V::Num(limit as f64)));
    }
    let auth = if key.is_empty() { Vec::new() } else { vec![("Authorization", format!("Bearer {key}"))] };
    let url = format!("{}/chat/completions", p.get("base_url").s().trim_end_matches('/'));
    let r = http::post_json(&url, &auth, &obj(body).dump(), timeout_of(p, 300)).map_err(|e| BackendError::new(format!("{name} unavailable: {e}")))?;
    if !(200..300).contains(&r.status) {
        let text = format!("{name} HTTP {}: {}", r.status, snippet(&r.body, 300));
        return Err(if r.status == 429 { BackendError::limited(text) } else { BackendError::new(text) });
    }
    let data = parse(&r.body).unwrap_or(V::Null);
    let text = data.get("choices").arr().first().map_or("", |c| c.get("message").get("content").s()).to_string();
    let usage = data.get("usage");
    let (input, output) = (usage.get("prompt_tokens").n(0.0) as u64, usage.get("completion_tokens").n(0.0) as u64);
    Ok(Reply {
        text,
        model: format!("{name}:{model}"),
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + output,
        estimated: !matches!(usage, V::Obj(_)),
        ..Reply::default()
    })
}

// ── Anthropic API with the user's own key. Rust has no official SDK, so this is raw HTTP. ──

const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Models that take the server-side refusal fallback.
const FALLBACK_MODELS: [&str; 4] = ["claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"];

fn anthropic(model: &str, system: &str, prompt: &str, cfg: &V, max_tokens: Option<u64>) -> Out {
    let a = cfg.get("anthropic");
    let env = a.get("api_key_env").s();
    let key = std::env::var(env).unwrap_or_default();
    if key.is_empty() {
        return Err(BackendError::new(format!("${env} is not set")));
    }
    let mut body = vec![
        ("model", st(model)),
        // Thinking counts against this limit, so leave room or the JSON gets cut off.
        ("max_tokens", V::Num(max_tokens.map_or(16000, |m| m.saturating_mul(3).max(1024)) as f64)),
        ("system", st(system)),
        ("messages", V::Arr(vec![obj(vec![("role", st("user")), ("content", st(prompt))])])),
    ];
    if !a.get("effort").s().is_empty() {
        body.push(("output_config", obj(vec![("effort", a.get("effort").clone())])));
    }
    let mut headers = vec![("x-api-key", key), ("anthropic-version", ANTHROPIC_VERSION.to_string())];
    if FALLBACK_MODELS.contains(&model) {
        headers.push(("anthropic-beta", "server-side-fallback-2026-07-01".to_string()));
        body.push(("fallbacks", st("default")));
    }
    let url = format!("{}/v1/messages", a.get("base_url").s().trim_end_matches('/'));
    let r = http::post_json(&url, &headers, &obj(body).dump(), 600).map_err(|e| BackendError::new(format!("anthropic unavailable: {e}")))?;
    let data = parse(&r.body).unwrap_or(V::Null);
    if !(200..300).contains(&r.status) {
        let detail = data.get("error").get("message").s();
        let text = format!("anthropic HTTP {}: {}", r.status, if detail.is_empty() { snippet(&r.body, 300) } else { detail.to_string() });
        return Err(if r.status == 429 { BackendError::limited(text) } else { BackendError::new(text) });
    }
    if data.get("stop_reason").s() == "refusal" {
        return Err(BackendError::new("anthropic declined this request"));
    }
    let text: String = data.get("content").arr().iter().filter(|b| b.get("type").s() == "text").map(|b| b.get("text").s()).collect();
    let u = data.get("usage");
    let input = (u.get("input_tokens").n(0.0) + u.get("cache_read_input_tokens").n(0.0) + u.get("cache_creation_input_tokens").n(0.0)) as u64;
    let output = u.get("output_tokens").n(0.0) as u64;
    Ok(Reply { text, model: format!("anthropic:{model}"), input_tokens: input, output_tokens: output, total_tokens: input + output, ..Reply::default() })
}

// ── echo (offline testing) ───────────────────────────────────────────────────────

fn echo(model: &str, system: &str, prompt: &str) -> Reply {
    let text = if prompt.contains("ROLLING SUMMARY TASK") || prompt.contains("HANDOFF TASK") {
        "- [echo] summary line 1\n- [echo] summary line 2".to_string()
    } else if prompt.contains("PROGRESS UPDATE TASK") {
        "# PROGRESS\n\n## Current state\n- [echo] updated\n".to_string()
    } else {
        let last: String = prompt.trim().lines().next_back().unwrap_or("").chars().take(120).collect();
        format!("[echo:{model}] received {} tokens. Last line: {last}", estimate(prompt))
    };
    let (i, o) = ((estimate(system) + estimate(prompt)) as u64, estimate(&text) as u64);
    Reply { text, model: format!("echo:{model}"), input_tokens: i, output_tokens: o, total_tokens: i + o, estimated: true, ..Reply::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn think_blocks_are_removed() {
        assert_eq!(strip_think("<think>a\nb</think>\n hi"), "hi");
        assert_eq!(strip_think("x<think>a</think>y<think>b</think>z"), "xyz");
        assert_eq!(strip_think("no closing <think> here"), "no closing <think> here");
    }

    #[test]
    fn specs_need_a_backend() {
        assert!(parse_spec("noprefix").is_err());
        assert_eq!(parse_spec("Gemini:x:y").ok(), Some(("gemini".to_string(), "x:y".to_string())));
    }
}
