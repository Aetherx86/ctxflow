//! The app's local companion: serves the page and performs model calls with this machine's keys.
//!
//! The page still runs the protocol in WASM. This only adds what a browser cannot do alone: reach
//! the Claude Code CLI and keep keys out of the browser. It listens on loopback only and spends
//! money on the caller's behalf, so every API call needs the token from the start-up URL.
use crate::backends::{self, ollama_auto};
use crate::ledger;
use crate::state::Project;
use crate::util::{find_in_path, hex, R};
use ctxflow_kernel::json::{obj, parse, st, V};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

const MAX_HEADER: usize = 64 * 1024;
const MAX_BODY: usize = 8 * 1024 * 1024;
const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";

const ASSETS: &[(&str, &str, &[u8])] = &[
    ("/", HTML, include_bytes!("../../hosts/web/index.html")),
    ("/index.html", HTML, include_bytes!("../../hosts/web/index.html")),
    ("/app.js", JS, include_bytes!("../../hosts/web/app.js")),
    ("/sw.js", JS, include_bytes!("../../hosts/web/sw.js")),
    ("/style.css", "text/css; charset=utf-8", include_bytes!("../../hosts/web/style.css")),
    ("/icon.svg", "image/svg+xml", include_bytes!("../../hosts/web/icon.svg")),
    ("/manifest.webmanifest", "application/manifest+json", include_bytes!("../../hosts/web/manifest.webmanifest")),
    ("/engine/index.mjs", JS, include_bytes!("../../hosts/js/dist/index.mjs")),
    ("/engine/kernel.wasm", "application/wasm", include_bytes!("../../hosts/js/dist/kernel.wasm")),
];

pub fn detect(cfg: &V) -> V {
    let has = |name: &str| std::env::var(name).is_ok_and(|v| !v.is_empty());
    let claude = cfg.get("claude").get("binary").s();
    obj(vec![
        ("gemini", V::Bool(has(cfg.get("gemini").get("api_key_env").s()))),
        ("local", V::Bool(ollama_auto(cfg.get("ollama").get("host").s()).is_ok())),
        ("claude", V::Bool(find_in_path(if claude.is_empty() { "claude" } else { claude }).is_some())),
        ("anthropic", V::Bool(has(cfg.get("anthropic").get("api_key_env").s()))),
        ("openrouter", V::Bool(has("OPENROUTER_API_KEY"))),
    ])
}

struct Shared {
    root: String,
    cfg: V,
    token: String,
    hosts: [String; 2],
    origins: [String; 2],
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEADER {
            return None;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_string(), first.next()?);
    let path = target.split(['?', '#']).next().unwrap_or("/").to_string();
    let headers: Vec<(String, String)> = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
    let length = headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    if length > MAX_BODY {
        return Some(Request { method, path, headers, body: Vec::new() }).filter(|_| {
            let _ = stream.write_all(b"HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            false
        });
    }
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some(Request { method, path, headers, body })
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &[u8]) {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        429 => "Too Many Requests",
        _ => "Bad Gateway",
    };
    let head = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
}

fn respond_json(stream: &mut TcpStream, code: u16, data: &V) {
    respond(stream, code, "application/json", data.dump().as_bytes());
}

fn error_json(stream: &mut TcpStream, code: u16, msg: &str) {
    respond_json(stream, code, &obj(vec![("error", st(msg))]));
}

/// Same length, then every byte compared, so timing does not reveal how much of a guess matched.
fn token_matches(given: &str, expected: &str) -> bool {
    given.len() == expected.len() && given.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn status(sh: &Shared) -> V {
    let cfg = &sh.cfg;
    let c = cfg.get("council");
    let mut profiles = Vec::new();
    if let V::Obj(all) = cfg.get("profiles") {
        for (name, p) in all {
            let fields = ["about", "seats", "chair", "fallbacks"].iter().map(|f| (f.to_string(), p.get(f).clone())).collect();
            profiles.push((name.clone(), V::Obj(fields)));
        }
    }
    obj(vec![
        ("version", st(env!("CARGO_PKG_VERSION"))),
        ("profiles", V::Obj(profiles)),
        ("active", cfg.get("active_profile").clone()),
        ("mode", cfg.get("mode").clone()),
        ("detect", detect(cfg)),
        ("today", V::Num(ledger::total_today(cfg) as f64)),
        ("limits", obj(vec![
            ("max_tokens", c.get("max_tokens").clone()),
            ("confirm_over", c.get("confirm_over").clone()),
            ("daily_tokens", c.get("daily_tokens").clone()),
        ])),
    ])
}

fn llm(stream: &mut TcpStream, sh: &Shared, body: &[u8]) {
    let Some(req) = std::str::from_utf8(body).ok().and_then(parse) else {
        return error_json(stream, 400, "bad request");
    };
    let (model, system, prompt) = (req.get("model").s(), req.get("system").s(), req.get("prompt").s());
    if model.is_empty() {
        return error_json(stream, 400, "bad request");
    }
    let cfg = &sh.cfg;
    let backend = model.split(':').next().unwrap_or("").to_ascii_lowercase();
    let local = backends::is_local(&backend, cfg);
    let daily = cfg.get("council").get("daily_tokens").n(0.0) as u64;
    if !local && daily > 0 && ledger::total_today(cfg) >= daily {
        return error_json(stream, 429, &format!("today's limit of {daily} tokens is used up"));
    }
    let max = req.get("max_tokens").n(500.0) as u64;
    if let (Some(reason), _) = ledger::check(cfg, model, prompt.len() as u64 / 4, max) {
        return error_json(stream, 429, &reason);
    }
    match backends::call(model, system, prompt, cfg, Some(max)) {
        Ok(r) => {
            let input = if r.input_tokens > 0 { r.input_tokens } else { prompt.len() as u64 / 4 };
            let output = if r.output_tokens > 0 { r.output_tokens } else { r.text.len() as u64 / 4 };
            ledger::record(model, input, output, &format!("app/{}", req.get("kind").s()), r.estimated, &sh.root, r.cost_usd);
            respond_json(stream, 200, &obj(vec![
                ("text", st(&r.text)),
                ("input", V::Num(input as f64)),
                ("output", V::Num(output as f64)),
                ("local", V::Bool(local)),
                ("removed", V::Num(r.redacted as f64)),
            ]));
        }
        Err(e) => error_json(stream, 502, &e.msg),
    }
}

fn handle(mut stream: TcpStream, sh: &Shared) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(req) = read_request(&mut stream) else { return };
    // A hostile page can reach localhost. Host stops DNS rebinding, Origin stops cross-site posts,
    // and the token stops anything that was not given the start-up URL.
    if !req.header("host").is_some_and(|h| sh.hosts.iter().any(|a| a == h)) {
        return error_json(&mut stream, 403, "bad host");
    }
    if req.header("origin").is_some_and(|o| !sh.origins.iter().any(|a| a == o)) {
        return error_json(&mut stream, 403, "bad origin");
    }
    if req.path.starts_with("/api/") && !token_matches(req.header("x-ctxflow-token").unwrap_or(""), &sh.token) {
        return error_json(&mut stream, 401, "bad token");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/status") => respond_json(&mut stream, 200, &status(sh)),
        ("POST", "/api/llm") => llm(&mut stream, sh, &req.body),
        ("GET", path) => match ASSETS.iter().find(|(p, _, _)| *p == path) {
            Some((_, ctype, body)) => respond(&mut stream, 200, ctype, body),
            None => error_json(&mut stream, 404, "not found"),
        },
        ("POST", _) => error_json(&mut stream, 404, "not found"),
        _ => error_json(&mut stream, 405, "method not allowed"),
    }
}

fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if result.is_err() {
        eprintln!("Open this address in a browser: {url}");
    }
}

pub fn serve(proj: &Project, cfg: V, port: u16, open: bool) -> R<()> {
    let mut secret = [0u8; 24];
    // The token guards money and keys, so a weak fallback is not acceptable here.
    getrandom::fill(&mut secret).map_err(|e| format!("no secure random source: {e}"))?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = hex(&secret);
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    let shared = Arc::new(Shared {
        root: proj.root.display().to_string(),
        cfg,
        token,
        hosts: [format!("127.0.0.1:{port}"), format!("localhost:{port}")],
        origins: [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}")],
    });
    println!("ctxflow app: {url}\nPress Ctrl+C to stop.");
    let _ = std::io::stdout().flush();
    if open {
        open_browser(&url);
    }
    for conn in listener.incoming().flatten() {
        let sh = Arc::clone(&shared);
        std::thread::spawn(move || handle(conn, &sh));
    }
    Ok(())
}
