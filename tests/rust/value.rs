use numera_gen_mcp::compat::value::{JsString, Number, Object, Value};

#[test]
fn authoritative_values_keep_infinity_and_utf16_across_clone_wire_and_conversion() {
    let value = Value::parse(r#"{"n":1e400,"\ud800":"\udfff","pair":"🌍"}"#).expect("parse");
    assert_eq!(value["n"].as_f64(), Some(f64::INFINITY));
    assert_eq!(
        value
            .as_object()
            .expect("object")
            .get_utf16(&[0xd800])
            .and_then(Value::as_utf16),
        Some([0xdfff].as_slice())
    );
    assert!(value.to_serde().is_err());
    assert_eq!(
        value.stringify().expect("wire"),
        r#"{"n":null,"\ud800":"\udfff","pair":"🌍"}"#
    );
    assert!(value.structurally_equal(&value.clone()));
    assert_eq!(value["pair"].as_str(), Some("🌍"));
    assert!(
        value
            .as_object()
            .expect("object")
            .get_utf16(&[0xd800])
            .expect("value")
            .as_str()
            .is_none()
    );
}

#[test]
fn object_mutation_iteration_entry_and_index_keep_exact_first_insertion() {
    let mut object = Object::new();
    object.insert("b".into(), Value::Bool(true));
    object.insert("a".into(), Value::Null);
    object.insert("b".into(), Value::Bool(false));
    object.insert(
        JsString::from_utf16(vec![0xd800]),
        Value::Number(Number::from(7.0)),
    );
    assert_eq!(object.keys().next().expect("first").as_str(), Some("b"));
    assert_eq!(object.len(), 3);
    object.entry("a").or_insert(Value::Bool(true));
    object.entry("c").or_insert(Value::Bool(true));
    assert!(object.get("a").expect("existing").is_null());
    for value in object.values_mut() {
        if value.as_bool() == Some(false) {
            *value = Value::Bool(true);
        }
    }
    assert_eq!(object.remove("b").expect("removed").as_bool(), Some(true));
    let value = Value::Object(object);
    assert!(value["missing"].is_null());
    assert!(value[0].is_null());
    let keys: Vec<_> = value
        .as_object()
        .expect("object")
        .keys()
        .map(JsString::as_utf16)
        .collect();
    assert_eq!(keys[0], [97]);
    assert_eq!(keys[1], [0xd800]);
    assert_eq!(keys[2], [99]);
    let mut value = Value::Null;
    value["created"] = Value::Array(vec![Value::Null]);
    value["created"][0] = Value::Bool(true);
    assert_eq!(value["created"][0].as_bool(), Some(true));
    assert!(Value::Bool(false).checked_index_mut("a").is_err());
    assert!(Value::Array(Vec::new()).checked_array_index_mut(0).is_err());
}

#[test]
fn serde_raw_json_boundaries_preserve_lone_units_but_serialize_nonfinite_as_null() {
    let text = r#"{"text":"\ud800","key":{"\udfff":1},"n":1e400}"#;
    let value: Value = serde_json::from_str(text).expect("raw decode");
    assert_eq!(value["n"].as_f64(), Some(f64::INFINITY));
    assert_eq!(value["text"].as_utf16(), Some([0xd800].as_slice()));
    assert_eq!(
        serde_json::to_string(&value).expect("raw encode"),
        r#"{"text":"\ud800","key":{"\udfff":1},"n":null}"#
    );
    let metadata = serde_json::json!({"valid": [1, "🌍"]});
    let value = Value::from_serde(&metadata).expect("metadata");
    assert_eq!(
        value.to_serde().expect("representable"),
        serde_json::json!({"valid":[1.0,"🌍"]})
    );
}

#[test]
fn cached_strings_never_replace_lone_units_and_numbers_follow_js_equality() {
    let valid = JsString::from("🌍");
    assert_eq!(valid.as_str(), Some("🌍"));
    assert_eq!(valid.as_utf16(), [0xd83c, 0xdf0d]);
    assert_eq!(valid.checked_str().expect("valid"), "🌍");
    let invalid = JsString::from_utf16(vec![0xd800]);
    assert!(invalid.as_str().is_none());
    assert_eq!(
        invalid.checked_str().expect_err("no replacement").code,
        "unrepresentable_json"
    );
    assert!(
        Value::Number(Number::from(-0.0)).structurally_equal(&Value::Number(Number::from(0.0)))
    );
    assert!(
        !Value::Number(Number::from(f64::NAN))
            .structurally_equal(&Value::Number(Number::from(f64::NAN)))
    );
    assert!(Number::from(f64::INFINITY).as_u64().is_none());
    assert_eq!(Number::from(7u32).as_u64(), Some(7));
}

#[test]
fn direct_macro_preserves_nested_js_values_without_a_serde_projection() {
    use numera_gen_mcp::compat::value::json;
    let lone = JsString::from_utf16(vec![0xd800]);
    let infinity = f64::INFINITY;
    let original = Value::parse(r#"{"\udfff":"\ud800","n":-1e400}"#).expect("original");
    let value =
        json!({"n": infinity, "text": lone, "items": [null, true, original, {"n": infinity}]});
    assert_eq!(value["n"].as_f64(), Some(f64::INFINITY));
    assert_eq!(value["text"].as_utf16(), Some([0xd800].as_slice()));
    assert_eq!(value["items"][2]["n"].as_f64(), Some(f64::NEG_INFINITY));
    assert_eq!(value["items"][3]["n"].as_f64(), Some(f64::INFINITY));
    let object = json!({(lone.clone()): infinity});
    assert_eq!(
        object
            .as_object()
            .expect("object")
            .get_utf16(&[0xd800])
            .and_then(Value::as_f64),
        Some(f64::INFINITY)
    );
    let optional: Option<&str> = None;
    let values = vec![1u64, 2];
    assert_eq!(
        json!({"none": optional, "values": values})["values"][1].as_u64(),
        Some(2)
    );
}

#[test]
fn deep_facade_lifecycle_and_mutation_are_iterative() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let depth = 8192;
            let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let value = Value::parse(&text).expect("deep");
            let mut copy = value.clone();
            assert!(value.structurally_equal(&copy));
            let mut leaf = &mut copy;
            for _ in 0..depth {
                leaf = &mut leaf[0];
            }
            *leaf = Value::Bool(true);
            assert!(!value.structurally_equal(&copy));
            assert_eq!(value.stringify().expect("deep wire"), text);
        })
        .expect("owned thread")
        .join()
        .expect("iterative lifecycle");
}

#[test]
fn serde_round_trip_and_every_to_value_conversion_keep_exact_shapes() {
    use numera_gen_mcp::compat::value::ToValue;
    use std::collections::{BTreeMap, HashMap};
    let original = Value::parse(r#"{"a":[1,"x",null,true],"b":{"c":2.5}}"#).unwrap();
    let text = serde_json::to_string(&original).unwrap();
    let back: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(back, original);
    assert!(serde_json::from_str::<Value>("{").is_err());
    assert_eq!(original.to_value(), original);
    assert_eq!("s".to_value(), Value::from("s"));
    assert_eq!(String::from("s").to_value(), Value::from("s".to_string()));
    assert_eq!(JsString::from("s").to_value(), Value::from(JsString::from("s")));
    assert_eq!(true.to_value(), Value::from(true));
    assert_eq!(Number::from(3u8).to_value().as_f64(), Some(3.0));
    assert!(().to_value().is_null());
    assert_eq!((&7u32).to_value().as_f64(), Some(7.0));
    assert!(None::<u8>.to_value().is_null());
    assert_eq!(Some(1i8).to_value().as_f64(), Some(1.0));
    assert_eq!([1u8, 2][..].to_value().as_array().unwrap().len(), 2);
    assert_eq!([1u16, 2, 3].to_value().as_array().unwrap().len(), 3);
    assert_eq!(vec![1i16].to_value().as_array().unwrap().len(), 1);
    let object = original.as_object().unwrap().clone();
    assert!(object.to_value().is_object());
    let hash: HashMap<&str, i32> = HashMap::from([("k", 1)]);
    assert_eq!(hash.to_value()["k"].as_f64(), Some(1.0));
    let tree: BTreeMap<String, i64> = BTreeMap::from([("z".to_string(), 2)]);
    assert_eq!(tree.to_value()["z"].as_f64(), Some(2.0));
    let js = numera_gen_mcp::compat::js_json::from_serde(&serde_json::json!({"q": 1})).unwrap();
    assert_eq!(js.to_value()["q"].as_f64(), Some(1.0));
    macro_rules! each {
        ($($v:expr),*) => {$(
            assert_eq!($v.to_value().as_f64(), Some(1.0));
            assert_eq!(Value::from($v).as_f64(), Some(1.0));
        )*};
    }
    each!(1f64, 1f32, 1i8, 1i16, 1i32, 1i64, 1isize, 1u8, 1u16, 1u32, 1u64, 1usize, 1i128, 1u128);
    let collected: Value = [Value::from(1u8), Value::from("x")].into_iter().collect();
    assert_eq!(collected.as_array().unwrap().len(), 2);
}
