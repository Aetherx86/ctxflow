//! Minimal JSON. The kernel has no dependencies, so there is nothing to audit but this file.
use alloc::{format, string::String, vec::Vec};

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<V>),
    Obj(Vec<(String, V)>),
}

impl V {
    pub fn get(&self, k: &str) -> &V {
        static NULL: V = V::Null;
        if let V::Obj(o) = self {
            for (a, b) in o {
                if a == k {
                    return b;
                }
            }
        }
        &NULL
    }
    pub fn s(&self) -> &str {
        if let V::Str(x) = self { x } else { "" }
    }
    pub fn n(&self, d: f64) -> f64 {
        if let V::Num(x) = self { *x } else { d }
    }
    pub fn b(&self) -> bool {
        matches!(self, V::Bool(true))
    }
    pub fn arr(&self) -> &[V] {
        if let V::Arr(x) = self { x } else { &[] }
    }
    pub fn set(&mut self, k: &str, v: V) {
        if let V::Obj(o) = self {
            for e in o.iter_mut() {
                if e.0 == k {
                    e.1 = v;
                    return;
                }
            }
            o.push((k.into(), v));
        }
    }
    pub fn dump(&self) -> String {
        let mut o = String::new();
        self.write(&mut o);
        o
    }
    fn write(&self, o: &mut String) {
        match self {
            V::Null => o.push_str("null"),
            V::Bool(b) => o.push_str(if *b { "true" } else { "false" }),
            V::Num(n) => {
                if *n == (*n as i64) as f64 {
                    o.push_str(&format!("{}", *n as i64));
                } else {
                    o.push_str(&format!("{:.3}", n));
                }
            }
            V::Str(s) => esc(s, o),
            V::Arr(a) => {
                o.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        o.push(',');
                    }
                    v.write(o);
                }
                o.push(']');
            }
            V::Obj(m) => {
                o.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        o.push(',');
                    }
                    esc(k, o);
                    o.push(':');
                    v.write(o);
                }
                o.push('}');
            }
        }
    }
}

fn esc(s: &str, o: &mut String) {
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
}

pub fn parse(t: &str) -> Option<V> {
    let mut p = P { b: t.as_bytes(), i: 0 };
    let v = p.val(0)?;
    p.ws();
    if p.i == p.b.len() { Some(v) } else { None }
}

/// Models wrap JSON in fences and prose. Take the outermost object that parses.
pub fn parse_loose(t: &str) -> Option<V> {
    if let Some(v) = parse(t.trim()) {
        return Some(v);
    }
    let a = t.find('{')?;
    let z = t.rfind('}')?;
    if z <= a {
        return None;
    }
    parse(&t[a..=z])
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\n' | b'\t' | b'\r') {
            self.i += 1;
        }
    }
    fn lit(&mut self, s: &str, v: V) -> Option<V> {
        if self.b[self.i..].starts_with(s.as_bytes()) {
            self.i += s.len();
            Some(v)
        } else {
            None
        }
    }
    fn val(&mut self, d: u32) -> Option<V> {
        if d > 64 {
            return None;
        }
        self.ws();
        match *self.b.get(self.i)? {
            b'n' => self.lit("null", V::Null),
            b't' => self.lit("true", V::Bool(true)),
            b'f' => self.lit("false", V::Bool(false)),
            b'"' => self.st().map(V::Str),
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if *self.b.get(self.i)? == b']' {
                    self.i += 1;
                    return Some(V::Arr(a));
                }
                loop {
                    a.push(self.val(d + 1)?);
                    self.ws();
                    match *self.b.get(self.i)? {
                        b',' => self.i += 1,
                        b']' => {
                            self.i += 1;
                            return Some(V::Arr(a));
                        }
                        _ => return None,
                    }
                }
            }
            b'{' => {
                self.i += 1;
                let mut m = Vec::new();
                self.ws();
                if *self.b.get(self.i)? == b'}' {
                    self.i += 1;
                    return Some(V::Obj(m));
                }
                loop {
                    self.ws();
                    let k = self.st()?;
                    self.ws();
                    if *self.b.get(self.i)? != b':' {
                        return None;
                    }
                    self.i += 1;
                    m.push((k, self.val(d + 1)?));
                    self.ws();
                    match *self.b.get(self.i)? {
                        b',' => self.i += 1,
                        b'}' => {
                            self.i += 1;
                            return Some(V::Obj(m));
                        }
                        _ => return None,
                    }
                }
            }
            _ => self.num(),
        }
    }
    fn num(&mut self) -> Option<V> {
        let s = self.i;
        while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
            self.i += 1;
        }
        core::str::from_utf8(&self.b[s..self.i]).ok()?.parse::<f64>().ok().map(V::Num)
    }
    fn hex4(&mut self) -> Option<u32> {
        let h = core::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }
    fn st(&mut self) -> Option<String> {
        if *self.b.get(self.i)? != b'"' {
            return None;
        }
        self.i += 1;
        let mut o: Vec<u8> = Vec::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(o).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    let r = match e {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'/' | b'\\' | b'"' => e as char,
                        b'u' => {
                            let mut u = self.hex4()?;
                            if (0xd800..0xdc00).contains(&u) && self.b.get(self.i..self.i + 2)? == b"\\u" {
                                self.i += 2;
                                let l = self.hex4()?;
                                u = 0x10000 + ((u - 0xd800) << 10) + (l.wrapping_sub(0xdc00) & 0x3ff);
                            }
                            char::from_u32(u).unwrap_or('\u{fffd}')
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    o.extend_from_slice(r.encode_utf8(&mut buf).as_bytes());
                }
                c => o.push(c),
            }
        }
    }
}

pub fn obj(items: Vec<(&str, V)>) -> V {
    V::Obj(items.into_iter().map(|(k, v)| (String::from(k), v)).collect())
}
pub fn st(s: &str) -> V {
    V::Str(String::from(s))
}
