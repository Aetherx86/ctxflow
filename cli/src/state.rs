//! Project-local state: session (rolling summary + recent turns) and markdown state docs.
//! The files keep the format of earlier versions, so existing projects continue to work.
use crate::config::{CONFIG_DIR, CONFIG_FILE, DEFAULT_TOML};
use crate::util::{hex, now, random_bytes, stamp, str_arr, strs, R};
use ctxflow_kernel::json::{obj, parse, st, V};
use std::fs;
use std::path::{Path, PathBuf};

const SPEC_TEMPLATE: &str = "# SPEC\n\n> Goals, constraints and design decisions that rarely change. The shorter this is, the fewer tokens every request uses.\n\n## Goals\n-\n\n## Constraints and stack\n-\n\n## Key decisions\n-\n";
const PROGRESS_TEMPLATE: &str = "# PROGRESS\n\n> The single source of truth for current state. This document is the memory, not the chat history.\n\n## Current state\n-\n\n## Done\n-\n\n## Next\n1.\n\n## Known issues\n-\n";

#[derive(Clone)]
pub struct Turn {
    pub user: String,
    pub assistant: String,
    pub model: String,
    pub ts: f64,
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub started: f64,
    pub summary: String,
    pub turns: Vec<Turn>,
    pub pending_progress: Vec<String>,
    pub spent_tokens: u64,
    pub advisor: V,
}

impl Session {
    pub fn new(summary: &str) -> Self {
        Session {
            id: hex(&random_bytes::<4>()),
            started: now(),
            summary: summary.to_string(),
            turns: Vec::new(),
            pending_progress: Vec::new(),
            spent_tokens: 0,
            advisor: V::Obj(Vec::new()),
        }
    }

    fn from_v(v: &V) -> Self {
        let advisor = v.get("advisor").clone();
        Session {
            id: v.get("id").s().to_string(),
            started: v.get("started").n(0.0),
            summary: v.get("summary").s().to_string(),
            turns: v.get("turns").arr().iter().map(|t| Turn {
                user: t.get("user").s().to_string(),
                assistant: t.get("assistant").s().to_string(),
                model: t.get("model").s().to_string(),
                ts: t.get("ts").n(0.0),
            }).collect(),
            pending_progress: strs(v.get("pending_progress")),
            spent_tokens: v.get("spent_tokens").n(0.0) as u64,
            advisor: if matches!(advisor, V::Obj(_)) { advisor } else { V::Obj(Vec::new()) },
        }
    }

    fn to_v(&self) -> V {
        obj(vec![
            ("id", st(&self.id)),
            ("started", V::Num(self.started)),
            ("summary", st(&self.summary)),
            ("turns", V::Arr(self.turns.iter().map(|t| obj(vec![
                ("user", st(&t.user)),
                ("assistant", st(&t.assistant)),
                ("model", st(&t.model)),
                ("ts", V::Num(t.ts)),
            ])).collect())),
            ("pending_progress", str_arr(&self.pending_progress)),
            ("spent_tokens", V::Num(self.spent_tokens as f64)),
            ("advisor", self.advisor.clone()),
        ])
    }
}

pub struct Project {
    pub root: PathBuf,
    pub dir: PathBuf,
}

impl Project {
    pub fn at(root: PathBuf) -> Self {
        let dir = root.join(CONFIG_DIR);
        Project { root, dir }
    }

    /// Searches upward. ~/.ctxflow holds the global ledger and is not a project, so the config file decides.
    pub fn find(start: &Path) -> Self {
        let cur = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
        for p in cur.ancestors() {
            if p.join(CONFIG_DIR).join(CONFIG_FILE).is_file() {
                return Project::at(p.to_path_buf());
            }
        }
        Project::at(cur)
    }

    pub fn initialized(&self) -> bool {
        self.dir.join(CONFIG_FILE).is_file()
    }

    pub fn init(&self, with_docs: bool) -> R<Vec<String>> {
        let mut created = Vec::new();
        let write = |path: &Path, text: &str| fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()));
        for sub in ["sessions", "backups"] {
            fs::create_dir_all(self.dir.join(sub)).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        }
        let cfg = self.dir.join(CONFIG_FILE);
        if !cfg.exists() {
            write(&cfg, DEFAULT_TOML)?;
            created.push(format!("{CONFIG_DIR}/{CONFIG_FILE}"));
        }
        let ignore = self.dir.join(".gitignore");
        if !ignore.exists() {
            write(&ignore, "session.json\nsessions/\nbackups/\narchive/\nprofile\nmode\n")?;
        }
        if with_docs {
            for (name, tpl) in [("SPEC.md", SPEC_TEMPLATE), ("PROGRESS.md", PROGRESS_TEMPLATE)] {
                let p = self.root.join(name);
                if !p.exists() {
                    write(&p, tpl)?;
                    created.push(name.to_string());
                }
            }
        }
        Ok(created)
    }

    fn session_path(&self) -> PathBuf {
        self.dir.join("session.json")
    }

    pub fn load_session(&self) -> Session {
        fs::read_to_string(self.session_path()).ok().and_then(|t| parse(&t)).map_or_else(|| Session::new(""), |v| Session::from_v(&v))
    }

    pub fn save_session(&self, s: &Session) -> R<()> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let tmp = self.dir.join("session.tmp");
        fs::write(&tmp, s.to_v().dump()).map_err(|e| e.to_string())?;
        fs::rename(&tmp, self.session_path()).map_err(|e| e.to_string())
    }

    pub fn archive_session(&self, s: &Session) -> R<PathBuf> {
        let dest = self.dir.join("sessions").join(format!("{}-{}.json", stamp("-"), s.id));
        fs::create_dir_all(self.dir.join("sessions")).map_err(|e| e.to_string())?;
        fs::write(&dest, s.to_v().dump()).map_err(|e| e.to_string())?;
        Ok(dest)
    }

    /// Originals of everything pruned from prompts, addressed by content hash.
    pub fn archive_put(&self, stubs: &[V]) -> R<()> {
        if stubs.is_empty() {
            return Ok(());
        }
        let d = self.dir.join("archive");
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&d, fs::Permissions::from_mode(0o700));
        }
        for stub in stubs {
            let path = d.join(stub.get("id").s());
            if !path.exists() {
                fs::write(&path, stub.get("body").s()).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn archive_get(&self, reference: &str) -> Option<String> {
        let ident = reference.trim_start_matches('#');
        if ident.is_empty() || !ident.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        fs::read_to_string(self.dir.join("archive").join(ident)).ok()
    }

    pub fn read_doc(&self, name: &str) -> String {
        fs::read_to_string(self.root.join(name)).unwrap_or_default()
    }

    pub fn write_doc(&self, name: &str, content: &str) -> R<()> {
        let p = self.root.join(name);
        if p.exists() {
            let backups = self.dir.join("backups");
            fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
            let stem = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            let ext = p.extension().map_or_else(String::new, |s| format!(".{}", s.to_string_lossy()));
            fs::copy(&p, backups.join(format!("{stem}.{}{ext}", stamp("-")))).map_err(|e| e.to_string())?;
        }
        fs::write(&p, format!("{}\n", content.trim_end())).map_err(|e| e.to_string())
    }
}
