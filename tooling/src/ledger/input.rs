use crate::context::fail;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Component, Path},
};

const LIMIT: usize = 16 * 1024 * 1024;

pub(super) fn relative(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name
            .bytes()
            .any(|byte| byte == b'\\' || byte == b':' || byte == 0)
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        Err(fail("Ledger path is not a contained public file."))
    } else {
        Ok(())
    }
}
pub(crate) fn read(root: &Path, name: &str) -> io::Result<Vec<u8>> {
    relative(name)?;
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for part in name.split('/') {
        path.push(part);
        let info = fs::symlink_metadata(&path)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if info.file_attributes() & 0x400 != 0 {
                return Err(fail("Linked ledger member rejected."));
            }
        }
        if info.is_symlink() {
            return Err(fail("Linked ledger member rejected."));
        }
    }
    if !path.canonicalize()?.starts_with(&root) {
        return Err(fail("Ledger member escapes root."));
    }
    // Reuse the maintained handle-verified regular-file reader, including per-platform link,
    // nonblocking/no-follow, one-link, identity and bounded-read checks.
    crate::distribution::read_regular(&path, LIMIT)
}
pub(super) fn text(bytes: &[u8]) -> io::Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| fail("Ledger evidence must be UTF-8."))
}
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Strict;
        impl<'de> Visitor<'de> for Strict {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| Unique(Value::Number(v)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<Unique>()? {
                    values.push(value.0);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    values.insert(key, map.next_value::<Unique>()?.0);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Strict)
    }
}
pub(crate) fn parse(bytes: &[u8]) -> io::Result<Value> {
    if bytes.len() > LIMIT {
        return Err(fail("Ledger JSON exceeds bound."));
    }
    serde_json::from_slice::<Unique>(text(bytes)?.as_bytes())
        .map(|v| v.0)
        .map_err(|_| fail("Invalid or duplicate-key ledger JSON."))
}
