use super::{
    finalization::locked,
    generation::Generation,
    identity::connection_identity,
    lease::Lease,
    media_request::{self, Request},
    media_requirements, media_wire,
};
use crate::{
    compat::{
        canonical_json::fingerprint,
        value::{Value, json},
    },
    config::snapshot_credentials,
    error::{NumeraError, Result},
    http::api::{Body, Operation},
    jobs::Receipt,
    media::fal,
};
use reqwest::Method;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn run(
    generation: &Generation,
    raw: Value,
    host_cancel: &CancellationToken,
) -> Result<Receipt> {
    let request = Request::parse(raw)?;
    let (name, selected) = generation.config.select(request.connection.as_deref())?;
    let model = request
        .model
        .as_deref()
        .or_else(|| selected["defaultModel"].as_str())
        .ok_or_else(|| media_request::invalid("Select a configured media model."))?
        .to_owned();
    let declaration = crate::config::media::declaration(&selected, &model.as_str().into())
        .ok_or_else(|| {
            media_request::invalid(
                "Media generation requires explicit model output types and modes.",
            )
        })?;
    if !crate::error::array(&declaration["modes"])
        .iter()
        .any(|v| v == request.mode.as_str())
    {
        return Err(media_request::invalid(
            "Selected model does not declare this media mode.",
        ));
    }
    if selected["adapter"] != "fal-queue" {
        return legacy(generation, request, host_cancel).await;
    }
    if selected["allowedModels"]
        .as_array()
        .is_some_and(|models| !models.iter().any(|value| value == model.as_str()))
        || selected["deniedModels"]
            .as_array()
            .is_some_and(|models| models.iter().any(|value| value == model.as_str()))
    {
        return Err(NumeraError::new(
            "model_unavailable",
            "Model is denied by connection policy.",
        ));
    }
    let effective = media_request::effective_parameters(&selected, &model, &request)?;
    // Contract validation precedes every source read/GET, including malformed native defaults.
    media_request::preflight(&model, &request, &effective, &generation.config.value)?;
    let cancel = host_cancel.child_token();
    let deadline = tokio::time::Instant::now()
        + Duration::from_millis(selected["requestTimeoutMs"].as_u64().unwrap_or(300000));
    let pinned = tokio::time::timeout_at(
        deadline,
        snapshot_credentials(&selected, &generation.env, &generation.runtime),
    )
    .await
    .map_err(|_| media_wire::deadline_error())??;
    let connection = pinned.value();
    let (references, hashes) = tokio::time::timeout_at(
        deadline,
        media_request::references(generation, &request, &cancel),
    )
    .await
    .map_err(|_| media_wire::deadline_error())??;
    let body = fal::prepare(
        &model,
        &request.mode,
        request.prompt.as_deref(),
        &references,
        &effective,
    )?;
    let identity = connection_identity(&name, &pinned)?;
    let parameters = Value::from_serde(&body)?;
    let hash = fingerprint(
        &json!({"contract":"fal-media-v1","identity":identity,"model":model,"mode":request.mode,"effective":parameters,"reference_hashes":Value::from_serde(&hashes)?,"output_subdirectory":request.output_subdirectory,"filename_prefix":request.filename_prefix}),
    )?;
    let id = request
        .request_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let expected_media = media_requirements::snapshot(&model, &effective)?;
    let initial = Receipt::new(
        json!({"request_id":id,"connection":name,"adapter":"fal-queue","operation":"media","mode":request.mode,
        "requested_model":model,"status":"prepared","outputs":[],"errors":[],"warnings":["Provider retention and billing are independent of local waiting; no refund is implied."],"deviations":[],
        "generation_outcome":"not_submitted","storage_outcome":"not_started","another_submission_may_charge":false,
        "output_requirements":{"output_subdirectory":request.output_subdirectory.as_deref().unwrap_or(&now[..10]),"filename_prefix":request.filename_prefix,"media":expected_media},"created_at":now,"updated_at":now}),
    );
    let config = &generation.config.value;
    let limits = crate::jobs::SubmissionLimits {
        queued: config["maxQueuedRequests"].as_u64().unwrap_or(16) as usize,
        global: config["maxConcurrentRequests"].as_u64().unwrap_or(4) as usize,
        connection: connection["maxConcurrentRequests"].as_u64().unwrap_or(2) as usize,
        bytes: config["files"]["maxAggregateBytes"]
            .as_i64()
            .unwrap_or(100 * 1024 * 1024),
        journal: config["files"]["maxJournalBytes"]
            .as_i64()
            .unwrap_or(256 * 1024 * 1024),
    };
    let prepared = locked(&generation.store)?.prepare_submission(initial, &hash, &identity, limits)?;
    let mut receipt = prepared.receipt;
    if !prepared.fresh {
        return Ok(receipt);
    }
    match generation.active.lock() {
        Ok(mut active) => {
            active.insert(id.clone(), cancel.clone());
        }
        Err(_) => {
            locked(&generation.store)?.release(&id, Some(&receipt))?;
            return Err(NumeraError::runtime());
        }
    }
    let lease = Lease::start(
        generation.store.clone(),
        receipt.clone(),
        cancel.clone(),
        true,
    );
    let action = async {
        let mut admitted = prepared.admission?;
        while !admitted {
            if cancel.is_cancelled() {
                return Err(super::generation::cancelled());
            }
            admitted = locked(&generation.store)?.admit_submission(&mut receipt, limits)?;
            if !admitted {
                super::generation::wait(&cancel, 50).await?;
            }
        }
        if cancel.is_cancelled() {
            return Err(super::generation::cancelled());
        }
        generation.logger.log(
            "INFO",
            "media",
            "Media submission intent persisted.",
            json!({"mode":request.mode}),
        );
        let (_, response) = media_wire::request_until(
            connection,
            &pinned.headers(),
            Operation {
                operation: &model,
                prefix: "",
                method: Method::POST,
                body: Body::Json(parameters),
                query: None,
            },
            &cancel,
            false,
            deadline,
        )
        .await?;
        let parsed = (|| {
            let handle = fal::handle(&model, &response)?;
            for (raw, suffix) in [
                (&handle.status_url, "status"),
                (&handle.response_url, "result"),
                (&handle.cancel_url, "cancel"),
            ] {
                media_wire::tracking(connection, &handle, raw, suffix)?;
            }
            Ok::<_, NumeraError>(handle)
        })();
        let handle = match parsed {
            Ok(handle) => handle,
            Err(_) => {
                // An unsafe acknowledgement is uncertain after POST; retain only a canonical ID,
                // never its untrusted tracking destinations and never a new submission.
                if let Some(id) = response.get("request_id")
                    && let Ok(handle) = fal::handle(&model, &serde_json::json!({"request_id":id}))
                {
                    receipt.value["upstream_job"] = media_wire::job(&handle)?;
                }
                receipt.value["acknowledgement_unverified"] = json!(true);
                receipt.value["status"] = json!("outcome_unknown");
                receipt.value["generation_outcome"] = json!("unknown");
                receipt.value["errors"].as_array_mut().ok_or_else(NumeraError::storage)?.push(json!(NumeraError::new("outcome_unknown","Submission acknowledgement could not be verified. Inspect only the original canonical handle; never resubmit.")));
                locked(&generation.store)?.update(&mut receipt)?;
                lease.update(&receipt);
                return Ok(());
            }
        };
        receipt.value["upstream_job"] = media_wire::job(&handle)?;
        locked(&generation.store)?.update(&mut receipt)?;
        receipt.value["status"] = json!("running");
        receipt.value["generation_outcome"] = json!("running");
        locked(&generation.store)?.update(&mut receipt)?;
        lease.update(&receipt);
        Ok::<(), NumeraError>(())
    };
    let result = match tokio::time::timeout_at(deadline, action).await {
        Ok(result) => result,
        Err(_) => {
            cancel.cancel();
            Err(media_wire::deadline_error())
        }
    };
    let result = (|| -> Result<Receipt> {
        match result {
            Ok(()) => Ok(receipt.clone()),
            Err(mut reason) => {
                if receipt.value["status"] == "submitting"
                    && receipt.value["generation_outcome"] == "unknown"
                    && matches!(
                        reason.code.as_str(),
                        "storage_error" | "unsafe_destination" | "permission_denied"
                    )
                {
                    reason=NumeraError::new("outcome_unknown","Submission intent exists but its acknowledgement could not be safely committed. Inspect the same request; never resubmit.").stage("submission");
                }
                let current = locked(&generation.store)?.get(&id)?;
                if !current.value["upstream_job"].is_null()
                    && current.value["generation_outcome"] == "unknown"
                    && locked(&generation.store)?.owns(&id, Some(&receipt))?
                {
                    let mut current = current;
                    current.value["status"] = json!("outcome_unknown");
                    current.value["acknowledgement_unverified"] = json!(true);
                    current.value["errors"]
                        .as_array_mut()
                        .ok_or_else(NumeraError::storage)?
                        .push(json!(reason));
                    locked(&generation.store)?.update(&mut current)?;
                    Ok(current)
                } else {
                    generation.record_error(&id, receipt.clone(), reason, cancel.is_cancelled())
                }
            }
        }
    })();
    lease.close().await;
    if let Ok(mut active) = generation.active.lock() {
        active.remove(&id);
    }
    locked(&generation.store)?.release(&id, None)?;
    let receipt = result?;
    if request.wait
        && receipt.value["upstream_job"]["kind"] == "fal"
        && receipt.status() != "outcome_unknown"
    {
        loop {
            if tokio::time::Instant::now() >= deadline
                || host_cancel.is_cancelled()
                || locked(&generation.store)?.cancelled(&id)?
            {
                return locked(&generation.store)?.get(&id);
            }
            let current =
                super::media_jobs::get_until(generation, &id, true, host_cancel, deadline).await?;
            if matches!(
                current.status(),
                "completed" | "partial" | "failed" | "cancelled"
            ) {
                return Ok(current);
            }
            tokio::select! {_=host_cancel.cancelled()=>return Ok(current),_=tokio::time::sleep_until((tokio::time::Instant::now()+Duration::from_millis(500)).min(deadline))=>{}}
        }
    }
    Ok(receipt)
}

async fn legacy(
    generation: &Generation,
    request: Request,
    cancel: &CancellationToken,
) -> Result<Receipt> {
    if !["text-to-image", "image-to-image"].contains(&request.mode.as_str()) {
        return Err(media_request::invalid(
            "This configured adapter implements only image modes.",
        ));
    }
    if request.mode == "text-to-image" && !request.references.is_empty() {
        return Err(media_request::invalid(
            "Text-to-image cannot silently consume typed media references; select image-to-image.",
        ));
    }
    let mut args = Value::from_serde(&request.parameters)?;
    for key in [
        "prompt",
        "request_id",
        "connection",
        "model",
        "reference_images",
        "wait",
        "output_subdirectory",
        "filename_prefix",
    ] {
        if args.get(key).is_some() {
            return Err(media_request::invalid(
                "Native image parameters conflict with typed media inputs.",
            ));
        }
    }
    args["prompt"] = json!(
        request
            .prompt
            .ok_or_else(|| media_request::invalid("Image generation needs a prompt."))?
    );
    if let Some(value) = request.request_id {
        args["request_id"] = json!(value);
    }
    if let Some(value) = request.connection {
        args["connection"] = json!(value);
    }
    if let Some(value) = request.model {
        args["model"] = json!(value);
    }
    if let Some(value) = request.output_subdirectory {
        args["output_subdirectory"] = json!(value);
    }
    if let Some(value) = request.filename_prefix {
        args["filename_prefix"] = json!(value);
    }
    args["wait"] = json!(request.wait);
    let mut references = Vec::new();
    for reference in request.references {
        if reference.kind != crate::media::Kind::Image || reference.role != "image" {
            return Err(media_request::invalid(
                "Legacy image reference role/kind cannot be forwarded.",
            ));
        }
        let (kind, value) = if let Some(v) = reference.output_id {
            ("output_id", v)
        } else if let Some(v) = reference.path {
            ("path", v)
        } else if let Some(v) = reference.url {
            ("url", v)
        } else {
            (
                "data_url",
                reference.data_url.ok_or_else(NumeraError::runtime)?,
            )
        };
        let mut source = json!({"type":kind});
        source[kind] = json!(value);
        references.push(source);
    }
    args["reference_images"] = json!(references);
    generation
        .run(
            args,
            if request.mode == "text-to-image" {
                "generate"
            } else {
                "edit"
            },
            cancel,
        )
        .await
}
