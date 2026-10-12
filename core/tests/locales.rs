//! `ui/locales/*.json` and the code that uses them stay in step: the same keys and `{name}`
//! placeholders in every language, every `tr` call in Rust, JS and Kotlin names an existing key
//! and passes exactly its arguments, and no key is left unused.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn locale(name: &str) -> BTreeMap<String, String> {
    let text = fs::read_to_string(root().join("ui/locales").join(format!("{name}.json"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn placeholders(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('}') {
            let name = &rest[..close];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.insert(name.to_string());
            }
        }
    }
    out
}

fn sources(dir: &str, ext: &str) -> Vec<(PathBuf, String)> {
    let mut out = vec![];
    let mut stack = vec![root().join(dir)];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push((p.clone(), fs::read_to_string(&p).unwrap()));
            }
        }
    }
    out
}

/// Does a string literal start at `j`? In JS any quote; in Rust and Kotlin a `'` is only a
/// char literal like '.' or '\n' (not a lifetime).
fn quote_at(s: &[u8], j: usize, js: bool) -> bool {
    match s[j] {
        b'"' | b'`' => true,
        b'\'' => js || s.get(j + 2) == Some(&b'\'') || s.get(j + 1) == Some(&b'\\'),
        _ => false,
    }
}

/// Index after the string literal starting at `i`, template `${}` included.
fn skip_string(s: &[u8], i: usize, js: bool) -> usize {
    let q = s[i];
    let mut j = i + 1;
    while j < s.len() {
        match s[j] {
            b'\\' => j += 2,
            c if c == q => return j + 1,
            b'$' if q != b'\'' && s.get(j + 1) == Some(&b'{') => j = skip_group(s, j + 1, js),
            _ => j += 1,
        }
    }
    s.len()
}

/// `s[i]` opens a group; index after the matching close.
fn skip_group(s: &[u8], i: usize, js: bool) -> usize {
    let mut depth = 0;
    let mut j = i;
    while j < s.len() {
        if quote_at(s, j, js) {
            j = skip_string(s, j, js);
            continue;
        }
        match s[j] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
        j += 1;
    }
    s.len()
}

/// The top-level comma-separated parts inside the group opening at `i`.
fn parts(src: &str, i: usize, js: bool) -> Vec<String> {
    let s = src.as_bytes();
    let end = skip_group(s, i, js);
    let mut out = vec![];
    let (mut depth, mut start, mut j) = (0, i + 1, i + 1);
    while j < end - 1 {
        if quote_at(s, j, js) {
            j = skip_string(s, j, js);
            continue;
        }
        match s[j] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(src[start..j].trim().to_string());
                start = j + 1;
            }
            _ => {}
        }
        j += 1;
    }
    let last = src[start..end - 1].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}

fn literal(arg: &str) -> Option<String> {
    let q = arg.chars().next()?;
    (matches!(q, '"' | '\'') && arg.len() >= 2 && arg.ends_with(q)).then(|| arg[1..arg.len() - 1].to_string())
}

struct Call {
    at: String,
    key: Option<String>,
    first: String,
    args: BTreeSet<String>,
}

/// `tr(`/`tr!(` calls of a file with their argument names.
fn calls(path: &Path, src: &str, lang: &str) -> Vec<Call> {
    let mut out = vec![];
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(off) = src[from..].find("tr") {
        let at = from + off;
        from = at + 2;
        let prev = src[..at].chars().last();
        if prev.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':' || c == '$') {
            continue;
        }
        let mut open = at + 2;
        let is_macro = bytes.get(open) == Some(&b'!');
        if is_macro {
            open += 1;
        }
        if bytes.get(open) != Some(&b'(') || (is_macro && lang != "rs") {
            continue;
        }
        // Definitions and comments are not calls.
        let line_start = src[..at].rfind('\n').map_or(0, |i| i + 1);
        let line = src[line_start..at].trim_start();
        if line.starts_with("//") || line == "*" || line.starts_with("* ") || line.starts_with("*/") || line.starts_with("/*") || line.ends_with("fn ") || line.ends_with("fun ") || line.ends_with("function ") || line.ends_with("macro_rules! ") {
            continue;
        }
        let p = parts(src, open, lang == "js");
        let lineno = src[..at].matches('\n').count() + 1;
        let mut call = Call { at: format!("{}:{lineno}", path.display()), key: p.first().and_then(|a| literal(a)), first: p.first().cloned().unwrap_or_default(), args: BTreeSet::new() };
        match lang {
            "rs" => {
                for a in &p[1..] {
                    call.args.insert(a.split('=').next().unwrap().trim().to_string());
                }
            }
            "js" => {
                if let Some(obj) = p.get(1) {
                    match obj.strip_prefix('{').map(|_| parts(obj, 0, true)) {
                        Some(names) => call.args.extend(names.iter().map(|a| a.split(':').next().unwrap().trim().to_string())),
                        None => {
                            call.args.insert(format!("<not an object literal: {obj}>"));
                        }
                    }
                }
            }
            _ => {
                for a in &p[1..] {
                    call.args.insert(literal(a.split(" to ").next().unwrap().trim()).expect("\"name\" to value"));
                }
            }
        }
        out.push(call);
    }
    out
}

#[test]
fn locales_match_each_other_and_the_code() {
    let ru = locale("ru");
    let en = locale("en");
    let mut errors = vec![];
    for k in ru.keys().filter(|k| !en.contains_key(*k)) {
        errors.push(format!("{k}: only in ru.json"));
    }
    for k in en.keys().filter(|k| !ru.contains_key(*k)) {
        errors.push(format!("{k}: only in en.json"));
    }
    for (k, r) in &ru {
        if let Some(e) = en.get(k) {
            if placeholders(r) != placeholders(e) {
                errors.push(format!("{k}: placeholders differ: {:?} / {:?}", placeholders(r), placeholders(e)));
            }
        }
    }

    let mut all = vec![];
    for (dir, ext, lang) in [
        ("core/src", "rs", "rs"),
        ("src-tauri/src", "rs", "rs"),
        ("ui/js", "js", "js"),
        ("android/app/src/main/java", "kt", "kt"),
    ] {
        for (p, src) in sources(dir, ext) {
            all.extend(calls(&p, &src, lang));
        }
    }
    assert!(all.len() > 500, "found only {} tr calls", all.len());
    let mut used = BTreeSet::new();
    for c in &all {
        let Some(key) = &c.key else {
            errors.push(format!("{}: the key must be a string literal (got {:?})", c.at, c.first));
            continue;
        };
        used.insert(key.clone());
        match en.get(key) {
            None => errors.push(format!("{}: no key {key}", c.at)),
            Some(text) if placeholders(text) != c.args => {
                errors.push(format!("{}: {key} takes {:?}, the call passes {:?}", c.at, placeholders(text), c.args));
            }
            _ => {}
        }
    }
    for k in en.keys().filter(|k| !used.contains(*k)) {
        errors.push(format!("{k}: not used anywhere"));
    }
    assert!(errors.is_empty(), "{} problem(s):\n{}", errors.len(), errors.join("\n"));
}

/// The UI fetches `ui/locales/*.json` from its own origin: the app's CSP must let it.
#[test]
fn csp_allows_loading_the_locales() {
    let conf: serde_json::Value = serde_json::from_str(&fs::read_to_string(root().join("src-tauri/tauri.conf.json")).unwrap()).unwrap();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap_or("");
    let connect = csp.split(';').map(str::trim).find(|d| d.starts_with("connect-src")).unwrap_or("");
    assert!(connect.split_whitespace().any(|s| s == "'self'"), "connect-src needs 'self' for ui/locales: {csp}");
}
