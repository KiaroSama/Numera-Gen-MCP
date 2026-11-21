use crate::{
    compat::value::Value,
    error::{NumeraError, Result},
    http::api::{self, Operation},
    media::fal,
};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

pub fn is_omniroute(connection: &Value) -> bool {
    connection["gateway"] == "omniroute"
}

pub fn tracking(
    connection: &Value,
    handle: &fal::Handle,
    raw: &str,
    suffix: &str,
) -> Result<String> {
    let base = url::Url::parse(
        connection["baseUrl"]
            .as_str()
            .ok_or_else(NumeraError::runtime)?,
    )
    .map_err(|_| NumeraError::new("invalid_configuration", "Invalid queue origin."))?;
    let raw_path = raw.split('?').next().unwrap_or(raw);
    if raw.len() > 4096
        || raw.chars().any(|c| c.is_control() || c == '\\')
        || ["%2e", "%2f", "%5c"]
            .iter()
            .any(|s| raw.to_ascii_lowercase().contains(s))
        || raw_path.split('/').any(|part| matches!(part, "." | ".."))
    {
        return Err(NumeraError::new(
            "permission_denied",
            "Existing queue URL contains an unsafe path encoding.",
        ));
    }
    let url = base
        .join(raw)
        .map_err(|_| NumeraError::new("permission_denied", "Invalid existing queue URL."))?;
    let fal_contract = connection["modelOverrides"]
        .get(handle.endpoint.as_str())
        .and_then(|entry| entry.get("falContract"));
    if is_omniroute(connection) {
        fal::gateway_handle_with_override(
            &handle.endpoint,
            fal_contract,
            &serde_json::json!({"id": handle.id}),
        )?;
        let expected_v1 = format!("/v1/fal/jobs/{}", handle.id);
        let expected_prefix = format!(
            "{}/fal/jobs/{}",
            base.path().trim_end_matches('/'),
            handle.id
        );
        let valid_query = match suffix {
            "result" => matches!(url.query(), None | Some("result=true")),
            "status" | "cancel" => url.query().is_none(),
            _ => return Err(NumeraError::runtime()),
        };
        if url.origin() != base.origin()
            || (url.path() != expected_v1 && url.path() != expected_prefix)
            || !url.username().is_empty()
            || url.password().is_some()
            || !valid_query
            || url.fragment().is_some()
        {
            return Err(NumeraError::new(
                "permission_denied",
                "Existing queue URL differs from its original destination/request; no credentials were sent.",
            ));
        }
        return Ok(format!("fal/jobs/{}", handle.id));
    }
    let expected = fal::handle_with_override(
        &handle.endpoint,
        fal_contract,
        &serde_json::json!({"request_id":handle.id}),
    )?;
    let expected = match suffix {
        "status" => expected.status_url,
        "cancel" => expected.cancel_url,
        "result" => expected.response_url,
        _ => return Err(NumeraError::runtime()),
    };
    if url.origin() != base.origin()
        || url.path() != expected
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(NumeraError::new(
            "permission_denied",
            "Existing queue URL differs from its original destination/request; no credentials were sent.",
        ));
    }
    Ok(url.path().trim_start_matches('/').to_owned())
}
pub fn deadline_error() -> NumeraError {
    NumeraError::new(
        "request_timeout",
        "Waiting stopped at the original deadline; inspect the existing job and never resubmit.",
    )
    .stage("network")
}

pub fn job(handle: &fal::Handle) -> Result<Value> {
    Ok(
        crate::compat::value::json!({"id":handle.id,"kind":"fal","handle":Value::from_serde(&serde_json::to_value(handle).map_err(|_|NumeraError::runtime())?)?}),
    )
}

pub async fn request_until(
    connection: &Value,
    headers: &HashMap<String, String>,
    wire: Operation<'_>,
    cancel: &CancellationToken,
    cancellation: bool,
    deadline: tokio::time::Instant,
) -> Result<(u16, serde_json::Value)> {
    if tokio::time::Instant::now() >= deadline {
        return Err(deadline_error());
    }
    let omniroute = is_omniroute(connection);
    let mut headers = headers.clone();
    if !omniroute {
        headers.insert("X-Fal-No-Retry".into(), "1".into());
        headers.insert("X-Fal-Store-IO".into(), "0".into());
    }
    let query = crate::compat::value::json!({"logs":"0"});
    let default_query =
        (!cancellation && !omniroute && wire.operation.ends_with("/status")).then_some(&query);
    let wire = Operation {
        query: wire.query.or(default_query),
        ..wire
    };
    let operation = async {
        if cancellation {
            api::request_statuses(connection, &headers, wire, cancel, 1024 * 1024, &[400, 404])
                .await
        } else {
            api::request(connection, &headers, wire, cancel, 16 * 1024 * 1024).await
        }
    };
    let response = tokio::time::timeout_at(deadline, operation)
        .await
        .map_err(|_| deadline_error())??;
    let value = serde_json::from_slice(&response.bytes).map_err(|_| {
        NumeraError::new(
            "invalid_response",
            "Queue returned invalid JSON; never resubmit automatically.",
        )
    })?;
    Ok((response.status, value))
}
pub fn stored(receipt: &crate::jobs::Receipt) -> Result<fal::Handle> {
    let handle: fal::Handle = serde_json::from_value(
        receipt.value["upstream_job"]["handle"].to_serde()?,
    )
    .map_err(|_| {
        NumeraError::new(
            "invalid_response",
            "Existing media job handle is incomplete.",
        )
    })?;
    if receipt.value["upstream_job"]["id"] != handle.id.as_str()
        || receipt.value["upstream_job"]["kind"] != "fal"
        || receipt.value["requested_model"] != handle.endpoint.as_str()
    {
        return Err(NumeraError::new(
            "permission_denied",
            "Existing media handle identity differs.",
        ));
    }
    Ok(handle)
}
