use crate::{
    compat::canonical_json::fingerprint,
    config::PinnedConnection,
    error::{NumeraError, Result},
};
use serde_json::json;

pub fn connection_identity(name: &str, connection: &PinnedConnection) -> Result<String> {
    let c = connection.value();
    let origin = url::Url::parse(c["baseUrl"].as_str().ok_or_else(NumeraError::runtime)?)
        .map_err(|_| NumeraError::runtime())?
        .origin()
        .ascii_serialization();
    let mut identity = json!({"connection":name,"adapter":c["adapter"],"origin":origin,"base":c["baseUrl"],"auth":c["auth"],
        "account_fingerprint":fingerprint(&serde_json::to_value(connection.headers()).map_err(|_|NumeraError::runtime())?)?});
    if let Some(gateway) = c.get("gateway") {
        identity["gateway"] = gateway.clone();
    }
    fingerprint(&identity)
}
