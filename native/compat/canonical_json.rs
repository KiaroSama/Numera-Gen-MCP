//! Source-runtime compatibility for existing persisted request fingerprints.
//! Remove only after an explicitly versioned identity migration and tested rollback.
use crate::error::{NumeraError, Result};
use icu_collator::{Collator, CollatorBorrowed, options::CollatorOptions};
use icu_locale::locale;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

pub fn fingerprint(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(canonical_json(value)?.as_bytes())))
}

pub fn canonical_json(value: &Value) -> Result<String> {
    let collator = Collator::try_new(locale!("en").into(), CollatorOptions::default())
        .map_err(|_| NumeraError::runtime())?;
    let mut output = String::new();
    write_value(value, &collator, &mut output)?;
    Ok(output)
}

fn index(key: &str) -> Option<u32> {
    let parsed = key.parse::<u32>().ok()?;
    (parsed != u32::MAX && parsed.to_string() == key).then_some(parsed)
}

fn write_value(value: &Value, collator: &CollatorBorrowed<'_>, output: &mut String) -> Result<()> {
    enum Token<'a> { Value(&'a Value), Key(&'a str), Character(char) }
    // Explicit traversal also handles valid deeply nested extension values without call-stack growth.
    let mut pending = vec![Token::Value(value)];
    while let Some(token) = pending.pop() {
        match token {
            Token::Character(c) => output.push(c),
            Token::Key(s) => output.push_str(&serde_json::to_string(s).map_err(|_| NumeraError::runtime())?),
            Token::Value(Value::Null) => output.push_str("null"),
            Token::Value(Value::Bool(b)) => output.push_str(if *b { "true" } else { "false" }),
            Token::Value(Value::Number(n)) => {
                // JSON.parse uses IEEE-754 even when serde retains an integer representation.
                let number = n.as_f64().ok_or_else(NumeraError::runtime)?;
                if number == 0.0 { output.push('0'); }
                else { output.push_str(ryu_js::Buffer::new().format(number)); }
            }
            Token::Value(Value::String(s)) => pending.push(Token::Key(s)),
            Token::Value(Value::Array(values)) => {
                output.push('[');
                pending.push(Token::Character(']'));
                for (i, v) in values.iter().enumerate().rev() {
                    pending.push(Token::Value(v));
                    if i != 0 { pending.push(Token::Character(',')); }
                }
            }
            Token::Value(Value::Object(values)) => {
                let mut keys: Vec<&str> = values.keys().map(String::as_str).collect();
                // Stable sorting preserves source insertion order for collator-equal keys.
                keys.sort_by(|a, b| collator.compare(a, b));
                // Object.fromEntries / JSON.stringify enumerate array-index keys first.
                keys.sort_by(|a, b| match (index(a), index(b)) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    _ => Ordering::Equal,
                });
                output.push('{');
                pending.push(Token::Character('}'));
                for (i, key) in keys.into_iter().enumerate().rev() {
                    pending.push(Token::Value(&values[key]));
                    pending.push(Token::Character(':'));
                    pending.push(Token::Key(key));
                    if i != 0 { pending.push(Token::Character(',')); }
                }
            }
        }
    }
    Ok(())
}
