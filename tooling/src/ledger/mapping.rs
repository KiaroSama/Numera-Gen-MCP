use super::input::{read, text};
use crate::context::fail;
use std::{collections::BTreeMap, io, path::Path};

pub(super) const SDK: [(&str, &str); 10] = [
    ("scripts/native-tools-acceptance.mjs", "toolsAcceptance"),
    (
        "scripts/native-two-host-acceptance.mjs",
        "twoHostAcceptance",
    ),
    ("scripts/native-compact-acceptance.mjs", "compactAcceptance"),
    ("scripts/native-env-acceptance.mjs", "envAcceptance"),
    (
        "scripts/native-negotiation-acceptance.mjs",
        "negotiationAcceptance",
    ),
    (
        "scripts/native-config-reload-acceptance.mjs",
        "configReloadAcceptance",
    ),
    (
        "scripts/native-terminal-acceptance.mjs",
        "terminalAcceptance",
    ),
    ("scripts/native-races-acceptance.mjs", "racesAcceptance"),
    (
        "scripts/native-lifecycle-acceptance.mjs",
        "lifecycleAcceptance",
    ),
    (
        "scripts/native-transport-acceptance.mjs",
        "transportAcceptance",
    ),
];
#[derive(Clone, Copy)]
pub(super) enum MappingPolicy {
    Rust,
    Historical,
    Reference,
}
#[derive(Clone)]
struct Function {
    name: String,
    test: bool,
}
// Mask comments and literals while preserving byte offsets/newlines. It prevents commented-out
// tests and strings containing fake attributes/functions from satisfying mapping registration.
fn code(text: &str) -> io::Result<String> {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut n = 0;
    while n < bytes.len() {
        let start = n;
        if bytes[n..].starts_with(b"//") {
            while n < bytes.len() && bytes[n] != b'\n' {
                n += 1;
            }
        } else if bytes[n..].starts_with(b"/*") {
            n += 2;
            let mut depth = 1;
            while n < bytes.len() && depth > 0 {
                if bytes[n..].starts_with(b"/*") {
                    depth += 1;
                    n += 2;
                } else if bytes[n..].starts_with(b"*/") {
                    depth -= 1;
                    n += 2;
                } else {
                    n += 1;
                }
            }
            if depth != 0 {
                return Err(fail("Unterminated native mapping comment."));
            }
        } else {
            let raw_start = if bytes[n] == b'r' {
                Some(n + 1)
            } else if bytes[n..].starts_with(b"br") || bytes[n..].starts_with(b"cr") {
                Some(n + 2)
            } else {
                None
            };
            let mut raw = None;
            if let Some(mut end) = raw_start {
                let hashes = end;
                while end < bytes.len() && bytes[end] == b'#' {
                    end += 1;
                }
                if bytes.get(end) == Some(&b'"') {
                    raw = Some((end + 1, end - hashes));
                }
            }
            if let Some((mut end, hashes)) = raw {
                let mut closed = false;
                while end < bytes.len() {
                    if bytes[end] == b'"'
                        && bytes
                            .get(end + 1..end + 1 + hashes)
                            .is_some_and(|s| s.iter().all(|b| *b == b'#'))
                    {
                        n = end + 1 + hashes;
                        closed = true;
                        break;
                    }
                    end += 1;
                }
                if !closed {
                    return Err(fail("Unterminated native mapping literal."));
                }
            } else if bytes[n] == b'"'
                || (bytes[n] == b'\''
                    && (bytes.get(n + 2) == Some(&b'\'') || bytes.get(n + 1) == Some(&b'\\')))
            {
                let quote = bytes[n];
                n += 1;
                let mut closed = false;
                while n < bytes.len() {
                    if bytes[n] == b'\\' {
                        n = (n + 2).min(bytes.len());
                    } else if bytes[n] == quote {
                        n += 1;
                        closed = true;
                        break;
                    } else {
                        n += 1;
                    }
                }
                if !closed {
                    return Err(fail("Unterminated native mapping literal."));
                }
            } else {
                n += 1;
                continue;
            }
        }
        for byte in &mut out[start..n] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(out).map_err(|_| fail("Native mapping source must be UTF-8."))
}
fn tokens(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            if !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
            if !ch.is_whitespace() {
                result.push(ch.to_string());
            }
        }
    }
    if !word.is_empty() {
        result.push(word);
    }
    result
}
fn functions(text: &str) -> io::Result<Vec<Function>> {
    let tokens = tokens(&code(text)?);
    let mut functions = Vec::new();
    let mut attributes = Vec::<Vec<String>>::new();
    let mut n = 0;
    let mut scopes = Vec::<bool>::new();
    let mut module_brace = false;
    // Test functions must be real module items. A matching token inside a function, impl,
    // macro definition/invocation or arbitrary block is not a registered test declaration.
    while n < tokens.len() {
        if tokens[n] == "{" {
            scopes.push(module_brace);
            module_brace = false;
            attributes.clear();
            n += 1;
            continue;
        }
        if tokens[n] == "}" {
            scopes.pop();
            attributes.clear();
            n += 1;
            continue;
        }
        if scopes.iter().any(|module| !*module) {
            attributes.clear();
            n += 1;
            continue;
        }
        if tokens[n] == "mod" && tokens.get(n + 2).is_some_and(|t| t == "{") {
            module_brace = true;
        }
        if tokens[n] == "#" && tokens.get(n + 1).is_some_and(|s| s == "[") {
            let mut end = n + 2;
            let mut depth = 1;
            while end < tokens.len() && depth > 0 {
                if tokens[end] == "[" {
                    depth += 1;
                }
                if tokens[end] == "]" {
                    depth -= 1;
                }
                end += 1;
            }
            if depth != 0 {
                return Err(fail("Invalid native test attribute."));
            }
            attributes.push(tokens[n + 2..end - 1].to_vec());
            n = end;
            continue;
        }
        if tokens[n] == "fn" {
            if let Some(name) = tokens.get(n + 1)
                && tokens.get(n + 2).is_some_and(|s| s == "(")
            {
                let test = attributes.iter().any(|a| {
                    (a.len() == 1 && a[0] == "test")
                        || (a
                            .iter()
                            .take(4)
                            .map(String::as_str)
                            .eq(["tokio", ":", ":", "test"])
                            && (a.len() == 4 || a.get(4).is_some_and(|token| token == "(")))
                });
                let ignored = attributes
                    .iter()
                    .any(|a| a.first().is_some_and(|s| s == "ignore"));
                if ignored && test {
                    return Err(fail("Ignored native mapping cannot prove execution."));
                }
                functions.push(Function {
                    name: name.clone(),
                    test,
                });
            }
            attributes.clear();
            n += 1;
            continue;
        }
        if ![
            "pub", "async", "unsafe", "extern", "(", ")", "crate", "super", "self",
        ]
        .contains(&tokens[n].as_str())
            && !tokens[n].starts_with("cfg")
        {
            attributes.clear();
        }
        n += 1;
    }
    Ok(functions)
}
pub(super) fn mapping<'a>(
    project: &Path,
    path: &str,
    separator: &str,
    symbol: &str,
    policy: MappingPolicy,
    cache: &'a mut BTreeMap<String, String>,
) -> io::Result<&'a str> {
    if separator != "::"
        || symbol.is_empty()
        || !symbol
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(fail("Native mapping requires an exact test symbol."));
    }
    if !cache.contains_key(path) {
        cache.insert(path.into(), text(&read(project, path)?)?.into());
    }
    let source = &cache[path];
    if let Some((_, registered)) = SDK.iter().find(|(file, _)| *file == path) {
        if !matches!(policy, MappingPolicy::Historical | MappingPolicy::Reference)
            || *registered != symbol
            || !source.contains(&format!("export async function {symbol}("))
        {
            return Err(fail(
                "Interpreted SDK mapping requires historical evidence, not maintained Rust execution.",
            ));
        }
        let caller = text(&read(project, "scripts/native-interop.mjs")?)?.to_owned();
        let name = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| fail("Invalid SDK fixture path."))?;
        if !caller.contains(&format!("import {{ {symbol} }} from \"./{name}\";"))
            || !caller.contains(&format!("await {symbol}("))
        {
            return Err(fail("Historical SDK fixture is not executed."));
        }
        for entry in ["scripts/native-ci.py", "scripts/native-extract-check.py"] {
            if !text(&read(project, entry)?)?.contains("scripts/native-interop.mjs") {
                return Err(fail(
                    "Historical SDK fixture lacks hosted/extracted execution.",
                ));
            }
        }
    } else {
        if !path.ends_with(".rs")
            || !(path.starts_with("native/")
                || path.starts_with("tests/rust/")
                || path.starts_with("tooling/tests/"))
        {
            return Err(fail("Native mapping must identify an actual Rust test."));
        }
        let found = functions(source)?
            .into_iter()
            .filter(|f| f.name == symbol)
            .collect::<Vec<_>>();
        if found.len() != 1 || !found[0].test {
            return Err(fail(
                "Mapped native symbol is missing, ambiguous or not an attributed test.",
            ));
        }
    }
    Ok(&cache[path])
}
pub fn mapping_source(
    project: &Path,
    path: &str,
    separator: &str,
    symbol: &str,
) -> io::Result<String> {
    Ok(mapping(
        project,
        path,
        separator,
        symbol,
        MappingPolicy::Rust,
        &mut BTreeMap::new(),
    )?
    .into())
}
pub fn historical_mapping_source(
    project: &Path,
    path: &str,
    separator: &str,
    symbol: &str,
) -> io::Result<String> {
    Ok(mapping(
        project,
        path,
        separator,
        symbol,
        MappingPolicy::Historical,
        &mut BTreeMap::new(),
    )?
    .into())
}
