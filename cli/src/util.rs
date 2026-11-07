//! Small helpers shared by the host. All decisions live in the kernel.
use ctxflow_kernel::compress::estimate;
use ctxflow_kernel::dispatch;
use ctxflow_kernel::json::{obj, parse, st, V};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type R<T> = Result<T, String>;

/// Calls the kernel the same way every other host does: JSON in, JSON out.
pub fn k(op: &str, mut args: Vec<(&str, V)>) -> V {
    args.push(("op", st(op)));
    parse(&dispatch(&obj(args).dump())).unwrap_or(V::Null)
}

pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// UTC, as std has no local time zone. Used for file names and notes.
pub fn stamp(sep: &str) -> String {
    let t = now() as i64;
    let (y, m, d) = civil_from_days(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    format!("{y:04}{m:02}{d:02}{sep}{:02}{:02}{:02}", s / 3600, s % 3600 / 60, s % 60)
}

pub fn dim(msg: &str) {
    if std::io::stderr().is_terminal() {
        eprintln!("\x1b[2m{msg}\x1b[0m");
    } else {
        eprintln!("{msg}");
    }
}

pub fn log(msg: &str) {
    dim(&format!("[ctxflow] {msg}"));
}

/// Printing to a closed pipe (`ctxflow show x | head`) must not crash the program.
pub fn say(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

pub fn strs(v: &V) -> Vec<String> {
    v.arr().iter().map(|x| x.s().to_string()).collect()
}

pub fn str_arr(items: &[String]) -> V {
    V::Arr(items.iter().map(|x| st(x)).collect())
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if getrandom::fill(&mut b).is_err() {
        // Only ids can land here with a weak value. The server token refuses this in `serve`.
        b.iter_mut().enumerate().for_each(|(i, x)| *x = (now() as u64 >> (i % 8)) as u8);
    }
    b
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
    let candidates = |dir: &Path| -> Option<PathBuf> {
        exts.iter().map(|e| dir.join(format!("{name}{e}"))).find(|p| p.is_file())
    };
    if name.contains('/') || name.contains('\\') {
        let p = PathBuf::from(name);
        return if p.is_file() { Some(p) } else { None };
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|d| candidates(&d))
}

pub struct ProcessOutput {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a command with a deadline. Output is read on threads so a full pipe cannot deadlock it.
pub fn run_process(bin: &Path, args: &[String], input: &str, cwd: Option<&Path>, timeout: Duration) -> R<ProcessOutput> {
    let mut cmd = Command::new(bin);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{} could not start: {e}", bin.display()))?;
    let mut stdin = child.stdin.take();
    let owned = input.to_string();
    let writer = std::thread::spawn(move || {
        if let Some(pipe) = stdin.as_mut() {
            let _ = pipe.write_all(owned.as_bytes());
        }
        drop(stdin);
    });
    let mut out_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let mut err_pipe = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} timed out", bin.display()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("{}: {e}", bin.display())),
        }
    };
    let _ = writer.join();
    let stdout = String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    Ok(ProcessOutput { ok: status.success(), code: status.code(), stdout, stderr })
}

pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> (String, bool) {
    if estimate(text) <= max_tokens {
        return (text.to_string(), false);
    }
    let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
    let (mut lo, mut hi) = (0usize, bounds.len() - 1);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if estimate(&text[..bounds[mid]]) <= max_tokens {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    (format!("{}\n… [ctxflow: truncated to fit token budget]", &text[..bounds[lo]]), true)
}
