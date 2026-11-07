//! `<project>/.ctxflow/config.toml` over the built-in defaults, so a partial file is fine.
use crate::util::{strs, R};
use ctxflow_kernel::json::{st, V};
use std::path::Path;

pub const DEFAULT_TOML: &str = include_str!("../default_config.toml");
pub const CONFIG_DIR: &str = ".ctxflow";
pub const CONFIG_FILE: &str = "config.toml";

fn from_toml(v: toml::Value) -> V {
    match v {
        toml::Value::String(s) => V::Str(s),
        toml::Value::Integer(i) => V::Num(i as f64),
        toml::Value::Float(f) => V::Num(f),
        toml::Value::Boolean(b) => V::Bool(b),
        toml::Value::Datetime(d) => V::Str(d.to_string()),
        toml::Value::Array(a) => V::Arr(a.into_iter().map(from_toml).collect()),
        toml::Value::Table(t) => V::Obj(t.into_iter().map(|(k, v)| (k, from_toml(v))).collect()),
    }
}

fn parse_toml(text: &str, what: &str) -> R<V> {
    let table: toml::Table = text.parse().map_err(|e| format!("{what}: {e}"))?;
    Ok(from_toml(toml::Value::Table(table)))
}

fn deep_merge(base: &mut V, over: V) {
    let V::Obj(items) = over else {
        *base = over;
        return;
    };
    for (key, val) in items {
        match base.get(&key) {
            V::Obj(_) if matches!(val, V::Obj(_)) => {
                let mut merged = base.get(&key).clone();
                deep_merge(&mut merged, val);
                base.set(&key, merged);
            }
            _ => base.set(&key, val),
        }
    }
}

pub fn load(root: &Path, profile: Option<&str>) -> R<V> {
    let mut cfg = parse_toml(DEFAULT_TOML, "built-in config")?;
    let path = root.join(CONFIG_DIR).join(CONFIG_FILE);
    if path.is_file() {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        deep_merge(&mut cfg, parse_toml(&text, &path.display().to_string())?);
    }
    let marker = |name: &str| std::fs::read_to_string(root.join(CONFIG_DIR).join(name)).unwrap_or_default().trim().to_string();
    let name = profile.map_or_else(|| marker("profile"), str::to_string);
    if !name.is_empty() {
        let prof = cfg.get("profiles").get(&name).clone();
        if !matches!(prof, V::Obj(_)) {
            let V::Obj(all) = cfg.get("profiles") else { return Err(format!("unknown profile {name:?}")) };
            let names: Vec<&str> = all.iter().map(|(k, _)| k.as_str()).collect();
            return Err(format!("unknown profile {name:?}. Available: {}", names.join(", ")));
        }
        let mut routing = cfg.get("routing").clone();
        deep_merge(&mut routing, prof.get("routing").clone());
        cfg.set("routing", routing);
    }
    cfg.set("active_profile", st(&name));
    let mode = marker("mode");
    cfg.set("mode", st(if mode.is_empty() { "auto" } else { &mode }));
    Ok(cfg)
}

pub fn model_budget<'a>(cfg: &'a V, spec: &str) -> &'a V {
    cfg.get("budget").get("models").get(spec)
}

/// Seats, chair and fallbacks of the active profile, or a cheap default from [routing].
pub fn council_seats(cfg: &V) -> (Vec<String>, String, Vec<String>) {
    let prof = cfg.get("profiles").get(cfg.get("active_profile").s());
    if matches!(prof, V::Obj(_)) {
        return (strs(prof.get("seats")), prof.get("chair").s().to_string(), strs(prof.get("fallbacks")));
    }
    let (light, heavy) = (strs(cfg.get("routing").get("light")), strs(cfg.get("routing").get("heavy")));
    let mut seats: Vec<String> = Vec::new();
    for m in light.iter().chain(heavy.iter()) {
        if !seats.contains(m) && seats.len() < 4 {
            seats.push(m.clone());
        }
    }
    (seats, heavy.first().cloned().unwrap_or_default(), Vec::new())
}
