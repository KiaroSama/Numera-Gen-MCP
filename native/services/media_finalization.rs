use super::{
    finalization::locked, generation::Generation, media_request::decode_data, media_requirements,
};
use crate::{
    compat::value::{Value, json},
    error::{NumeraError, Result},
    files::output,
    http::assets,
    jobs::Receipt,
    media::{self, Kind, fal::Artifact},
};
use tokio_util::sync::CancellationToken;

fn check_reported_mime(reported: Option<&str>, verified: &str) -> Result<()> {
    if let Some(mime) = reported {
        let mime = mime.split(';').next().unwrap_or("").trim();
        let matches = mime.eq_ignore_ascii_case(verified)
            || (verified == "audio/wav" && mime.eq_ignore_ascii_case("audio/x-wav"))
            || (verified == "image/jpeg" && mime.eq_ignore_ascii_case("image/jpg"));
        if !mime.is_empty() && !mime.eq_ignore_ascii_case("application/octet-stream") && !matches {
            return Err(NumeraError::new(
                "invalid_response",
                "Provider media MIME differs from the validated original bytes.",
            ));
        }
    }
    Ok(())
}

fn error(
    receipt: &mut Receipt,
    reason: NumeraError,
    index: u32,
    output_id: Option<&str>,
) -> Result<()> {
    let mut value = json!(reason);
    value["artifact_index"] = json!(index);
    if let Some(output_id) = output_id {
        value["output_id"] = json!(output_id);
    }
    let errors = receipt.value["errors"]
        .as_array_mut()
        .ok_or_else(NumeraError::storage)?;
    if !errors.contains(&value) {
        errors.push(value);
    }
    Ok(())
}

fn recover_errors(receipt: &mut Receipt, resolved: impl Fn(&Value) -> bool) -> Result<()> {
    let errors = receipt.value["errors"]
        .as_array_mut()
        .ok_or_else(NumeraError::storage)?;
    let mut recovered = Vec::new();
    errors.retain(|error| {
        if resolved(error) {
            recovered.push(error.clone());
            false
        } else {
            true
        }
    });
    if recovered.is_empty() {
        return Ok(());
    }
    if receipt.value["recovered_errors"].is_null() {
        receipt.value["recovered_errors"] = json!([]);
    }
    let history = receipt.value["recovered_errors"]
        .as_array_mut()
        .ok_or_else(NumeraError::storage)?;
    for error in recovered {
        if !history.contains(&error) {
            history.push(error);
        }
    }
    Ok(())
}

fn recover_published(receipt: &mut Receipt) -> Result<()> {
    // Publication commits the output and its journal phase atomically. A crash before the final
    // receipt update must not leave the already published original's download error active.
    let published: Vec<String> = crate::error::array(&receipt.value["outputs"])
        .iter()
        .filter_map(|output| output["output_id"].as_str().map(str::to_owned))
        .collect();
    recover_errors(receipt, |error| {
        error["output_id"]
            .as_str()
            .is_some_and(|id| published.iter().any(|saved| saved == id))
    })
}
pub async fn finalize(
    generation: &Generation,
    mut receipt: Receipt,
    artifacts: Option<Vec<Artifact>>,
    cancel: &CancellationToken,
    observe_cancel: bool,
) -> Result<Receipt> {
    let id = receipt.id()?.to_owned();
    let config = &generation.config.value;
    let max = config["files"]["maxOutputBytes"]
        .as_u64()
        .unwrap_or(50 * 1024 * 1024) as usize;
    let aggregate = config["files"]["maxAggregateBytes"]
        .as_i64()
        .unwrap_or(100 * 1024 * 1024);
    let pixels = config["files"]["maxPixels"].as_u64().unwrap_or(64000000);
    locked(&generation.store)?.reserve(
        &id,
        aggregate,
        config["files"]["maxJournalBytes"]
            .as_i64()
            .unwrap_or(256 * 1024 * 1024),
    )?;
    if !locked(&generation.store)?.owns(&id, Some(&receipt))? {
        return Err(NumeraError::storage());
    }
    receipt.value["status"] = json!("finalizing");
    receipt.value["storage_outcome"] = json!("writing");
    receipt.value["generation_outcome"] = if receipt.value["upstream_terminal"].is_null() {
        json!("completed")
    } else {
        receipt.value["upstream_terminal"].clone()
    };
    recover_published(&mut receipt)?;
    // Indexed errors are resolved only after their own original is published. Preserve legacy
    // non-storage errors; retry retained originals only, never a generation.
    if let Some(errors) = receipt.value["errors"].as_array_mut() {
        errors.retain(|v| {
            v["artifact_index"].as_u64().is_some()
                || !matches!(
                    v["code"].as_str(),
                    Some(
                        "output_file_error"
                            | "storage_error"
                            | "cancelled"
                            | "request_timeout"
                            | "outcome_unknown"
                    )
                )
        });
    }
    if let Some(artifacts) = artifacts {
        if artifacts.is_empty() || artifacts.len() > 64 {
            return Err(NumeraError::new(
                "invalid_response",
                "Media artifact count exceeds the shared durable journal bound.",
            ));
        }
        receipt.value["media_items"]=json!(artifacts.iter().map(|a|json!({"kind":a.kind.as_str(),"role":a.role,"reported_mime":a.reported_mime})).collect::<Vec<_>>());
        locked(&generation.store)?.update(&mut receipt)?;
        // Inline completion bytes are retained before observing a later wait cancellation.
        for (index, artifact) in artifacts.into_iter().enumerate() {
            let result = (|| {
                if artifact.error.is_some() {
                    return Err(NumeraError::new(
                        "invalid_response",
                        "Provider returned an invalid media artifact.",
                    ));
                }
                if artifact.url.starts_with("data:") {
                    let bytes = decode_data(&artifact.url, artifact.kind, max)?;
                    let inspected = media::inspect(artifact.kind, &bytes, max, pixels)?;
                    let mime = artifact
                        .url
                        .split_once(',')
                        .and_then(|(head, _)| head.strip_prefix("data:"))
                        .and_then(|head| head.strip_suffix(";base64"));
                    if mime != Some(inspected.mime.as_str()) {
                        return Err(NumeraError::new(
                            "invalid_response",
                            "Media data URI MIME differs from the validated bytes.",
                        ));
                    }
                    check_reported_mime(artifact.reported_mime.as_deref(), &inspected.mime)?;
                    locked(&generation.store)?.retain(&id, index as u32, &bytes, Some(&receipt))?;
                } else {
                    locked(&generation.store)?.retain_url(
                        &id,
                        index as u32,
                        &artifact.url.into(),
                        Some(&receipt),
                    )?;
                }
                Ok(())
            })();
            if let Err(reason) = result {
                error(&mut receipt, reason, index as u32, None)?;
            }
        }
    }
    locked(&generation.store)?.update(&mut receipt)?;
    let items = locked(&generation.store)?.pending(&id)?;
    for item in items {
        let action = async {
            if cancel.is_cancelled()
                || (observe_cancel && locked(&generation.store)?.cancelled(&id)?)
            {
                return Err(super::generation::cancelled());
            }
            let descriptor = &receipt.value["media_items"][item.index as usize];
            let kind: Kind = serde_json::from_value(descriptor["kind"].to_serde()?)
                .map_err(|_| NumeraError::storage())?;
            let role = descriptor["role"]
                .as_str()
                .ok_or_else(NumeraError::storage)?;
            let (bytes, inspected) = if let Some(bytes) = item.bytes {
                let inspected = media::inspect(kind, &bytes, max, pixels)?;
                (bytes, inspected)
            } else {
                let bytes = assets::fetch(
                    item.url.as_ref().ok_or_else(NumeraError::storage)?,
                    config,
                    max,
                    cancel,
                )
                .await?;
                let inspected = media::inspect(kind, &bytes, max, pixels)?;
                check_reported_mime(descriptor["reported_mime"].as_str(), &inspected.mime)?;
                locked(&generation.store)?.retain(&id, item.index, &bytes, Some(&receipt))?;
                if cancel.is_cancelled()
                    || (observe_cancel && locked(&generation.store)?.cancelled(&id)?)
                {
                    return Err(super::generation::cancelled());
                }
                (bytes, inspected)
            };
            let deviations = media_requirements::deviations(&receipt, &inspected, role, item.index);
            output::save_media(
                kind,
                role,
                &bytes,
                config,
                &mut *locked(&generation.store)?,
                output::Publication {
                    id: &id,
                    index: item.index,
                    subdir: receipt.value["output_requirements"]["output_subdirectory"].as_str(),
                    prefix: receipt.value["output_requirements"]["filename_prefix"].as_str(),
                    deviations: &deviations,
                    expected: Some(&receipt),
                    stable: true,
                },
            )?;
            Ok::<(), NumeraError>(())
        }
        .await;
        if let Err(reason) = action {
            if !locked(&generation.store)?.owns(&id, Some(&receipt))? {
                return Err(reason);
            }
            error(&mut receipt, reason, item.index, Some(&item.output_id))?;
            if cancel.is_cancelled()
                || (observe_cancel && locked(&generation.store)?.cancelled(&id)?)
            {
                break;
            }
        } else {
            let current = locked(&generation.store)?.get(&id)?;
            receipt.value["outputs"] = current.value["outputs"].clone();
            receipt.value["deviations"] = current.value["deviations"].clone();
            recover_errors(&mut receipt, |error| {
                error["artifact_index"].as_u64() == Some(u64::from(item.index))
            })?;
        }
    }
    media_requirements::reconcile_count(&mut receipt)?;
    let pending = locked(&generation.store)?.has_pending(&id)?;
    let count = crate::error::array(&receipt.value["outputs"]).len();
    if !pending && count > 0 {
        // Old receipts lack artifact indexes. Resolve their download errors conservatively:
        // every retained original must have been published; unrelated errors remain active.
        recover_errors(&mut receipt, |error| {
            error["artifact_index"].is_null()
                && error["code"] == "input_file_error"
                && error["stage"] == "download"
        })?;
    }
    let errors = !crate::error::array(&receipt.value["errors"]).is_empty();
    let deviations = !crate::error::array(&receipt.value["deviations"]).is_empty();
    receipt.value["status"] = json!(if count == 0 && pending {
        "finalizing"
    } else if count == 0 {
        "failed"
    } else if pending || errors || deviations {
        "partial"
    } else {
        "completed"
    });
    receipt.value["storage_outcome"] = json!(if pending || errors {
        "partial_or_failed"
    } else {
        "completed"
    });
    locked(&generation.store)?.update(&mut receipt)?;
    generation.logger.log(
        "INFO",
        "media",
        "Media originals processed.",
        json!({"count":count,"retained":pending,"status":receipt.status()}),
    );
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_is_artifact_scoped_deduplicated_and_preserves_history() {
        let mut receipt = Receipt::new(json!({"errors":[],"outputs":[]}));
        let failure = || NumeraError::new("input_file_error", "Unavailable.").stage("download");
        error(&mut receipt, failure(), 0, Some("first")).unwrap();
        error(&mut receipt, failure(), 0, Some("first")).unwrap();
        error(&mut receipt, failure(), 1, Some("second")).unwrap();
        assert_eq!(crate::error::array(&receipt.value["errors"]).len(), 2);
        recover_published(&mut receipt).unwrap();
        assert!(receipt.value["recovered_errors"].is_null());
        receipt.value["outputs"] = json!([{"output_id":"first"}]);
        recover_published(&mut receipt).unwrap();
        assert_eq!(crate::error::array(&receipt.value["errors"]).len(), 1);
        assert_eq!(receipt.value["errors"][0]["artifact_index"], 1);
        assert_eq!(receipt.value["recovered_errors"][0]["artifact_index"], 0);
        let previous = receipt.value.clone();
        recover_published(&mut receipt).unwrap();
        assert_eq!(receipt.value, previous);
    }

    #[test]
    fn recovered_publication_never_clears_an_unpublished_validation_failure() {
        let mut receipt = Receipt::new(json!({"errors":[],"outputs":[{"output_id":"first"}]}));
        error(
            &mut receipt,
            NumeraError::new("input_file_error", "Unavailable.").stage("download"),
            0,
            Some("first"),
        )
        .unwrap();
        error(
            &mut receipt,
            NumeraError::new("invalid_response", "Malformed sibling."),
            1,
            None,
        )
        .unwrap();
        recover_published(&mut receipt).unwrap();
        assert_eq!(crate::error::array(&receipt.value["errors"]).len(), 1);
        assert_eq!(receipt.value["errors"][0]["code"], "invalid_response");
        assert_eq!(
            crate::error::array(&receipt.value["recovered_errors"]).len(),
            1
        );
    }
}
