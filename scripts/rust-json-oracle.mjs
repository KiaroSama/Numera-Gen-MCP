// Keep raw JSON text: serializing the parsed input would erase nonfinite numbers before comparison.
export async function jsonOracle({ emit, fingerprint }) {
  const texts = [
    ["overflow-values", '{"a":1e400,"b":-1e400,"c":1e-400}'],
    ["lone-surrogate-value", '{"text":"\\ud800"}'],
    ["lone-surrogate-key", '{"\\udfff":"key","ordinary":1}'],
    ["paired-surrogate", '{"text":"\\ud83c\\udf0d"}'],
    ["duplicate-keys", '{"a":1,"b":2,"a":3}'],
    ["binary64-rounded-integer", '{"number":9007199254740993}'],
  ];
  const stable = (value) =>
    Array.isArray(value)
      ? value.map(stable)
      : value && typeof value === "object"
        ? Object.fromEntries(
            Object.entries(value)
              .sort(([a], [b]) => a.localeCompare(b, "en"))
              .map(([key, item]) => [key, stable(item)]),
          )
        : value;
  const cases = texts.map(([name, text]) => {
    const value = JSON.parse(text);
    return {
      name,
      text,
      serialized: JSON.stringify(value),
      canonical: JSON.stringify(stable(value)),
      sha256: fingerprint(value),
    };
  });
  await emit("raw-json", cases);
}
