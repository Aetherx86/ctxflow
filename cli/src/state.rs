//! Project-local state: session (rolling summary + recent turns) and markdown state docs.
//! The files keep the format of earlier versions, so existing projects continue to work.
use crate::config::{CONFIG_DIR, CONFIG_FILE, DEFAULT_TOML};
use crate::util::{fs_meta_is_link, hex, log, now, random_bytes, stamp, str_arr, strs, write_new, R};
use ctxflow_kernel::json::{obj, parse, st, V};
use std::fs;
use std::path::{Component, Path, PathBuf};

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
            let dir = p.join(CONFIG_DIR);
            if !fs_meta_is_link(&dir) && !fs_meta_is_link(&dir.join(CONFIG_FILE)) && dir.join(CONFIG_FILE).is_file() {
                return Project::at(p.to_path_buf());
            }
        }
        Project::at(cur)
    }

    pub fn initialized(&self) -> bool {
        !fs_meta_is_link(&self.dir) && !fs_meta_is_link(&self.dir.join(CONFIG_FILE)) && self.dir.join(CONFIG_FILE).is_file()
    }

    /// A path inside the project, given as a plain relative name, with no symlink anywhere on the
    /// way. A cloned repository can plant `SPEC.md -> ~/.ssh/id_rsa` or `PROGRESS.md -> ~/.zshrc`;
    /// those are refused instead of read and sent, or overwritten.
    pub fn inside(&self, rel: &str) -> R<PathBuf> {
        let rel_path = Path::new(rel);
        if rel.is_empty() || !rel_path.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(format!("{rel:?} must be a plain relative path inside the project"));
        }
        let mut cur = self.root.clone();
        for part in rel_path.components() {
            cur.push(part);
            if fs_meta_is_link(&cur) {
                return Err(format!("{} is a symlink; ctxflow does not follow links in a project", cur.display()));
            }
        }
        Ok(cur)
    }

    pub fn init(&self, with_docs: bool) -> R<Vec<String>> {
        if fs_meta_is_link(&self.dir) {
            return Err(format!("{} is a symlink", self.dir.display()));
        }
        let mut created = Vec::new();
        let write = |path: &Path, text: &str| write_new(path, text.as_bytes());
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
                let p = self.inside(name)?;
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
        if fs_meta_is_link(&self.session_path()) {
            return Session::new("");
        }
        fs::read_to_string(self.session_path()).ok().and_then(|t| parse(&t)).map_or_else(|| Session::new(""), |v| Session::from_v(&v))
    }

    pub fn save_session(&self, s: &Session) -> R<()> {
        self.inside(CONFIG_DIR)?;
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        // Our own temporary name: anything already there, a planted link included, is replaced.
        let tmp = self.dir.join("session.tmp");
        write_new(&tmp, s.to_v().dump().as_bytes())?;
        fs::rename(&tmp, self.session_path()).map_err(|e| e.to_string())
    }

    pub fn archive_session(&self, s: &Session) -> R<PathBuf> {
        self.inside(".ctxflow/sessions")?;
        fs::create_dir_all(self.dir.join("sessions")).map_err(|e| e.to_string())?;
        let dest = self.inside(&format!(".ctxflow/sessions/{}-{}-{}.json", stamp("-"), s.id, hex(&random_bytes::<2>())))?;
        write_new(&dest, s.to_v().dump().as_bytes())?;
        Ok(dest)
    }

    /// Originals of everything pruned from prompts, addressed by content hash.
    pub fn archive_put(&self, stubs: &[V]) -> R<()> {
        if stubs.is_empty() {
            return Ok(());
        }
        let d = self.inside(".ctxflow/archive")?;
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&d, fs::Permissions::from_mode(0o700));
        }
        for stub in stubs {
            let id = stub.get("id").s();
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
                continue;
            }
            let path = self.inside(&format!(".ctxflow/archive/{id}"))?;
            if !path.exists() {
                write_new(&path, stub.get("body").s().as_bytes())?;
            }
        }
        Ok(())
    }

    pub fn archive_get(&self, reference: &str) -> Option<String> {
        let ident = reference.trim_start_matches('#');
        if ident.is_empty() || !ident.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        fs::read_to_string(self.inside(&format!(".ctxflow/archive/{ident}")).ok()?).ok()
    }

    pub fn read_doc(&self, name: &str) -> String {
        match self.inside(name) {
            Ok(p) => fs::read_to_string(p).unwrap_or_default(),
            Err(e) => {
                log(&format!("skipped {name}: {e}"));
                String::new()
            }
        }
    }

    pub fn write_doc(&self, name: &str, content: &str) -> R<()> {
        let p = self.inside(name)?;
        if p.exists() {
            self.inside(".ctxflow/backups")?;
            fs::create_dir_all(self.dir.join("backups")).map_err(|e| e.to_string())?;
            let stem = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            let ext = p.extension().map_or_else(String::new, |s| format!(".{}", s.to_string_lossy()));
            let backup = self.inside(&format!(".ctxflow/backups/{stem}.{}-{}{ext}", stamp("-"), hex(&random_bytes::<2>())))?;
            write_new(&backup, &fs::read(&p).map_err(|e| e.to_string())?)?;
        }
        write_new(&p, format!("{}\n", content.trim_end()).as_bytes())
    }
}
