use super::{
    finalization::{self, SharedStore, locked},
    identity::connection_identity,
    lease::Lease,
    model_policy, request, requirements,
};
use crate::{
    adapters::{
        self,
        types::{Input, Normalized},
    },
    capabilities,
    compat::canonical_json::fingerprint,
    config::{Config, snapshot_credentials},
    error::{NumeraError, Result},
    files::{editing, images, output, paths},
    http::{
        api::{self, Body},
        assets,
    },
    jobs::Receipt,
    logging::Logger,
    runtime_paths::RuntimePaths,
};
use reqwest::Method;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub struct Generation {
    pub config: Arc<Config>,
    pub store: SharedStore,
    pub logger: Arc<Logger>,
    pub runtime: RuntimePaths,
    pub env: HashMap<String, String>,
    pub active: Arc<Mutex<HashMap<String, CancellationToken>>>,
}
impl Generation {
    pub async fn source(
        &self,
        source: &Value,
        cancel: &CancellationToken,
    ) -> Result<images::Image> {
        let config = &self.config.value;
        let limit = config["files"]["maxInputBytes"]
            .as_u64()
            .unwrap_or(20 * 1024 * 1024) as usize;
        if source["type"] == "output_id" {
            let (_, image) = output::owned(
                config,
                &*locked(&self.store)?,
                source["output_id"]
                    .as_str()
                    .ok_or_else(NumeraError::runtime)?,
            )?;
            if image.bytes.len() > limit {
                return Err(NumeraError::new(
                    "invalid_input",
                    "Owned output exceeds reference byte limit.",
                ));
            }
            return Ok(image);
        }
        let bytes = match source["type"].as_str() {
            Some("path") => paths::input(
                source["path"].as_str().ok_or_else(NumeraError::runtime)?,
                &crate::error::array(&config["files"]["allowedInputRoots"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from)
                    .collect::<Vec<_>>(),
                limit,
            )?,
            Some("data_url") => images::data_url(
                source["data_url"]
                    .as_str()
                    .ok_or_else(NumeraError::runtime)?,
                limit,
            )?,
            Some("url") => {
                assets::fetch(
                    source["url"].as_str().ok_or_else(NumeraError::runtime)?,
                    config,
                    limit,
                    cancel,
                )
                .await?
            }
            _ => {
                return Err(NumeraError::new(
                    "invalid_input",
                    "Invalid reference source.",
                ));
            }
        };
        images::inspect(bytes, config, Some(limit))
    }

    pub async fn run(
        &self,
        raw: Value,
        operation: &str,
        host_cancel: &CancellationToken,
    ) -> Result<Receipt> {
        let original = editing::request(request::parse(raw)?, operation)?;
        let (name, selected) = self.config.select(original["connection"].as_str())?;
        let pinned = snapshot_credentials(&selected, &self.env, &self.runtime).await?;
        let c = pinned.value();
        let headers = pinned.headers();
        let model = original["model"]
            .as_str()
            .or_else(|| c["defaultModel"].as_str())
            .ok_or_else(|| {
                NumeraError::new(
                    "invalid_input",
                    "Select a model or configure this connection defaultModel.",
                )
            })?;
        let preliminary = capabilities::effective_request(c, &original, model)?;
        let (request, policy_warnings) =
            model_policy::validate(c, &headers, model, preliminary, host_cancel).await?;
        let id = request["request_id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let sources = crate::error::array(&request["reference_images"]);
        let config = &self.config.value;
        if sources.len() as u64 > config["files"]["maxReferences"].as_u64().unwrap_or(14) {
            return Err(NumeraError::new(
                "invalid_input",
                "Reference count exceeds configured input limit.",
            ));
        }
        if operation == "edit" && sources.is_empty() {
            return Err(NumeraError::new(
                "invalid_input",
                "edit_image requires at least one reference.",
            ));
        }
        if let Some(path) = request["output_subdirectory"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            paths::safe_relative(path)?;
        }
        if let Some(prefix) = request["filename_prefix"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            paths::safe_relative(prefix)?;
            if prefix.contains(['/', '\\']) {
                return Err(NumeraError::new(
                    "invalid_input",
                    "Filename prefix cannot include directories.",
                ));
            }
        }
        if request["wait"] == false && c["adapter"] != "comfyui" {
            return Err(NumeraError::new(
                "unsupported_operation",
                "wait=false requires a recoverable ComfyUI job.",
            ));
        }
        let cancel = host_cancel.child_token();
        let timeout = Duration::from_millis(c["requestTimeoutMs"].as_u64().unwrap_or(300000));
        let deadline = tokio::time::Instant::now() + timeout;
        let mut references = Vec::new();
        let mut total = 0usize;
        for source in sources {
            let image = tokio::time::timeout_at(deadline, self.source(source, &cancel))
                .await
                .map_err(|_| deadline_error())??;
            total = total.checked_add(image.bytes.len()).ok_or_else(|| {
                NumeraError::new("invalid_input", "Aggregate reference bytes exceeded.")
            })?;
            if total as u64
                > config["files"]["maxAggregateBytes"]
                    .as_u64()
                    .unwrap_or(100 * 1024 * 1024)
            {
                return Err(NumeraError::new(
                    "invalid_input",
                    "Aggregate reference bytes exceeded.",
                ));
            }
            references.push(image);
        }
        let mut upscale_source = None;
        if request["upscale"] == true {
            let (w, h) = request["size"]
                .as_str()
                .and_then(editing::dimensions)
                .ok_or_else(NumeraError::runtime)?;
            let image = references.first().ok_or_else(NumeraError::runtime)?;
            if u64::from(w) * u64::from(h)
                > config["files"]["maxPixels"].as_u64().unwrap_or(64000000)
                || w < image.width
                || h < image.height
                || (w == image.width && h == image.height)
            {
                return Err(NumeraError::new(
                    "invalid_input",
                    "Upscale size must enlarge the reference without shrinking either dimension and fit the pixel limit.",
                ));
            }
            upscale_source = Some(json!({"width":image.width,"height":image.height}));
        }
        let mask = if let Some(mask) = request.get("mask") {
            Some(
                tokio::time::timeout_at(deadline, self.source(mask, &cancel))
                    .await
                    .map_err(|_| deadline_error())??,
            )
        } else if request.get("edit_region").is_some() {
            Some(editing::region_mask(
                &request,
                references.first().ok_or_else(|| {
                    NumeraError::new("invalid_input", "Mask needs a reference image.")
                })?,
                c,
                config,
            )?)
        } else {
            None
        };
        if let Some(mask) = &mask {
            if total.checked_add(mask.bytes.len()).is_none_or(|n| {
                n as u64
                    > config["files"]["maxAggregateBytes"]
                        .as_u64()
                        .unwrap_or(100 * 1024 * 1024)
            }) {
                return Err(NumeraError::new(
                    "invalid_input",
                    "Aggregate reference and mask bytes exceeded.",
                ));
            }
            images::validate_mask(
                mask,
                references.first().ok_or_else(|| {
                    NumeraError::new("invalid_input", "Mask needs a reference image.")
                })?,
            )?;
        }
        let input = Input {
            connection: c,
            request: &request,
            model,
            operation,
            references: &references,
            mask: mask.as_ref(),
        };
        let built = if c["adapter"] == "comfyui" {
            None
        } else {
            Some(adapters::build_request(&input)?)
        };
        let workflow = if c["adapter"] == "comfyui" {
            Some(
                tokio::time::timeout_at(
                    deadline,
                    adapters::comfy::prepare(&input, &headers, &cancel),
                )
                .await
                .map_err(|_| deadline_error())??,
            )
        } else {
            None
        };
        let identity = connection_identity(&name, &pinned)?;
        let mut parameters = request
            .as_object()
            .cloned()
            .ok_or_else(NumeraError::runtime)?;
        for key in [
            "request_id",
            "return_mode",
            "wait",
            "reference_images",
            "mask",
        ] {
            parameters.remove(key);
        }
        let mut hashed = json!({"identity":identity,"model":model,"operation":operation,"prompt":request["prompt"],"count":request["count"],"parameters":parameters,
            "reference_hashes":references.iter().map(|i|&i.sha256).collect::<Vec<_>>()});
        if let Some(mask) = &mask {
            hashed["mask_hash"] = json!(mask.sha256);
        }
        if let Some(workflow) = c.get("workflow") {
            hashed["workflow"] = workflow.clone();
        }
        let hash = fingerprint(&hashed)?;
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut required = json!({"count":request["count"],"output_subdirectory":request["output_subdirectory"].as_str().unwrap_or(&now[..10])});
        for key in ["size", "output_format", "background", "filename_prefix"] {
            if let Some(value) = request.get(key) {
                required[key] = value.clone();
            }
        }
        if let Some(source) = upscale_source {
            required["upscale_source"] = source;
        }
        if let (Some(object), Some(values)) = (
            required.as_object_mut(),
            requirements::snapshot(c, model, &request).as_object(),
        ) {
            object.extend(values.clone());
        }
        let mut warnings = vec![json!(
            "Gateway/provider internal retries and fallback are outside Numera control."
        )];
        warnings.extend(policy_warnings.into_iter().map(Value::String));
        let initial = Receipt::new(
            json!({"request_id":id,"connection":name,"adapter":c["adapter"],"operation":operation,"status":"prepared","requested_model":model,
            "upstream_model":null,"outputs":[],"warnings":warnings,"deviations":[],"errors":[],"generation_outcome":"not_submitted","storage_outcome":"not_started",
            "another_submission_may_charge":true,"output_requirements":required,"created_at":now,"updated_at":now}),
        );
        let (fresh, mut receipt) = locked(&self.store)?.prepare(initial, &hash, &identity)?;
        if !fresh {
            return Ok(receipt);
        }
        self.active
            .lock()
            .map_err(|_| NumeraError::runtime())?
            .insert(id.clone(), cancel.clone());
        let lease = Lease::start(self.store.clone(), receipt.clone(), cancel.clone(), true);
        let result = tokio::time::timeout_at(
            deadline,
            self.submit(
                &input,
                built,
                workflow,
                &headers,
                &cancel,
                &id,
                &name,
                &mut receipt,
                &lease,
                &identity,
            ),
        )
        .await;
        let result = match result {
            Ok(v) => v,
            Err(_) => Err(NumeraError::new(
                "request_timeout",
                "Waiting stopped; upstream completion and billing may be unknown.",
            )
            .stage("network")),
        };
        let result = match result {
            Ok(receipt) => Ok(receipt),
            Err(error) => self.record_error(&id, receipt, error, cancel.is_cancelled()),
        };
        lease.close().await;
        if let Ok(mut active) = self.active.lock() {
            active.remove(&id);
        }
        if let Ok(receipt) = &result {
            locked(&self.store)?.release(&id, Some(receipt))?;
        } else {
            locked(&self.store)?.release(&id, None)?;
        }
        result
    }

    async fn submit(
        &self,
        input: &Input<'_>,
        built: Option<adapters::types::Prepared>,
        workflow: Option<Value>,
        headers: &HashMap<String, String>,
        cancel: &CancellationToken,
        id: &str,
        name: &str,
        receipt: &mut Receipt,
        lease: &Lease,
        identity: &str,
    ) -> Result<Receipt> {
        let c = input.connection;
        let config = &self.config.value;
        if locked(&self.store)?.queued()? as u64
            > config["maxQueuedRequests"].as_u64().unwrap_or(16)
        {
            return Err(NumeraError::new(
                "rate_limited",
                "Local bounded queue is full.",
            ));
        }
        while !locked(&self.store)?.admit(
            id,
            name,
            config["maxConcurrentRequests"].as_u64().unwrap_or(4) as usize,
            if c["adapter"] == "comfyui" {
                1
            } else {
                c["maxConcurrentRequests"].as_u64().unwrap_or(2) as usize
            },
        )? {
            tokio::select! {_=cancel.cancelled()=>return Err(cancelled()),_=tokio::time::sleep(Duration::from_millis(50))=>{}}
        }
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        locked(&self.store)?.reserve(
            id,
            config["files"]["maxAggregateBytes"]
                .as_i64()
                .unwrap_or(100 * 1024 * 1024),
            config["files"]["maxJournalBytes"]
                .as_i64()
                .unwrap_or(256 * 1024 * 1024),
        )?;
        receipt.value["status"] = json!("submitting");
        receipt.value["generation_outcome"] = json!("unknown");
        locked(&self.store)?.update(receipt)?;
        self.logger.log(
            "INFO",
            "generation",
            "Submission intent persisted.",
            json!({"requestId":id,"connection":name}),
        );
        let mut claimed = false;
        let result = if let Some(workflow) = workflow {
            let (job, warnings) = adapters::comfy::submit(input, workflow, headers, cancel).await?;
            receipt.value["upstream_job"] = json!({"id":job.id,"kind":job.kind});
            receipt.value["warnings"]
                .as_array_mut()
                .ok_or_else(NumeraError::runtime)?
                .extend(warnings.into_iter().map(Value::String));
            receipt.value["status"] = json!("running");
            receipt.value["generation_outcome"] = json!("running");
            locked(&self.store)?.update(receipt)?;
            if input.request["wait"] == false {
                return Ok(receipt.clone());
            }
            loop {
                let shared = Mutex::new(receipt.clone());
                let status = adapters::comfy::status(
                    c,
                    headers,
                    &job.id,
                    cancel,
                    config["files"]["maxOutputBytes"]
                        .as_u64()
                        .unwrap_or(50 * 1024 * 1024) as usize,
                    config["files"]["maxAggregateBytes"]
                        .as_u64()
                        .unwrap_or(100 * 1024 * 1024) as usize,
                    |index, bytes| {
                        let mut current = shared.lock().map_err(|_| NumeraError::storage())?;
                        if !claimed {
                            if !locked(&self.store)?.claim(id, Some(&current), false)? {
                                return Err(NumeraError::new(
                                    "outcome_unknown",
                                    "Another waiter owns output finalization.",
                                )
                                .stage("storage"));
                            }
                            *current = locked(&self.store)?.get(id)?;
                            lease.update(&current);
                            claimed = true;
                        }
                        finalization::retain(
                            config,
                            &self.store,
                            id,
                            index,
                            bytes.to_vec(),
                            &current,
                        )?;
                        Ok(())
                    },
                    || {
                        let mut current = shared.lock().map_err(|_| NumeraError::storage())?;
                        current.value["generation_outcome"] = json!("completed");
                        locked(&self.store)?.update(&mut current)
                    },
                )
                .await;
                *receipt = shared.into_inner().map_err(|_| NumeraError::storage())?;
                let status = status?;
                if let Some(terminal) = status.terminal {
                    return locked(&self.store)?.terminate(
                        id,
                        terminal,
                        &json!(
                            NumeraError::new("provider_rejection", "ComfyUI workflow failed.")
                                .stage("generation")
                        ),
                        Some(receipt),
                        Some(&json!({"id":job.id,"kind":job.kind})),
                    );
                }
                if let Some(result) = status.result {
                    break result;
                }
                wait(cancel, 500).await?;
            }
        } else {
            let built = built.ok_or_else(NumeraError::runtime)?;
            let response = api::request(
                c,
                headers,
                &built.operation,
                built.prefix,
                Method::POST,
                built.body,
                None,
                cancel,
                config["files"]["maxAggregateBytes"]
                    .as_u64()
                    .unwrap_or(100 * 1024 * 1024) as usize,
            )
            .await?;
            if let Some(id) = response.request_id {
                receipt.value["upstream_request_id"] = json!(id);
            }
            let mut normalized = adapters::normalize::response(c, response.bytes, &response.mime)?;
            if let Some(job) = &normalized.job {
                receipt.value["upstream_job"] = json!({"id":job.id,"kind":job.kind});
                receipt.value["status"] = json!("running");
                locked(&self.store)?.update(receipt)?;
            }
            while let Some(job) = normalized.job.take() {
                wait(cancel, 500).await?;
                let raw = api::json(
                    c,
                    headers,
                    &format!("{}/{}", job.kind, adapters::encode_component(&job.id)),
                    built.prefix,
                    Method::GET,
                    Body::Empty,
                    None,
                    cancel,
                )
                .await?;
                normalized = adapters::normalize::normalize(c, &raw)?;
                validate_job(&normalized, &job)?;
            }
            normalized
        };
        if !claimed {
            if !locked(&self.store)?.claim(id, Some(receipt), false)? {
                return locked(&self.store)?.get(id);
            }
            *receipt = locked(&self.store)?.get(id)?;
            lease.update(receipt);
        }
        let continuation = result.continuation.clone();
        let mut completed = self
            .finish(receipt.clone(), result, input.request, cancel, false)
            .await?;
        if let Some(value) = continuation {
            let stored = locked(&self.store)?.save_continuation(
                id,
                &fingerprint(&json!({"identity":identity,"model":input.model}))?,
                &value,
            );
            if let Err(error) = stored {
                if !locked(&self.store)?.owns(id, Some(&completed))? {
                    return locked(&self.store)?.get(id);
                }
                completed.value["warnings"].as_array_mut().ok_or_else(NumeraError::runtime)?.push(json!("Private continuation metadata could not be retained; original outputs remain available."));
                locked(&self.store)?.update(&mut completed)?;
                self.logger.log(
                    "WARNING",
                    "generation",
                    "Continuation storage failed after retaining outputs.",
                    json!({"code":error.code}),
                );
            }
        }
        Ok(completed)
    }

    pub async fn finish(
        &self,
        mut receipt: Receipt,
        result: Normalized,
        request: &Value,
        cancel: &CancellationToken,
        recovery: bool,
    ) -> Result<Receipt> {
        let id = receipt.id()?.to_owned();
        if let Some(state) = &result.terminal {
            if !locked(&self.store)?.owns(&id, Some(&receipt))? {
                return locked(&self.store)?.get(&id);
            }
            let job = receipt.value.get("upstream_job").cloned();
            receipt = locked(&self.store)?.terminate(
                &id,
                state,
                &json!(
                    NumeraError::new(
                        "provider_rejection",
                        "Provider reported a terminal generation result."
                    )
                    .stage("response")
                ),
                Some(&receipt),
                job.as_ref(),
            )?;
            if result.images.is_empty() && !locked(&self.store)?.has_pending(&id)? {
                return Ok(receipt);
            }
        }
        finalization::finalize(
            &self.config.value,
            &self.store,
            &self.logger,
            receipt,
            Some(result),
            request,
            cancel,
            recovery,
        )
        .await
    }

    fn record_error(
        &self,
        id: &str,
        mut receipt: Receipt,
        error: NumeraError,
        aborted: bool,
    ) -> Result<Receipt> {
        if !locked(&self.store)?.owns(id, Some(&receipt))? {
            return locked(&self.store)?.get(id);
        }
        receipt = locked(&self.store)?.get(id)?;
        let ambiguous = receipt.status() == "submitting"
            && matches!(
                error.code.as_str(),
                "outcome_unknown"
                    | "request_timeout"
                    | "invalid_response"
                    | "cancelled"
                    | "no_image_returned"
            );
        receipt.value["status"] = json!(if !crate::error::array(&receipt.value["outputs"])
            .is_empty()
        {
            "partial"
        } else if ambiguous {
            "outcome_unknown"
        } else if aborted {
            "cancelled"
        } else {
            "failed"
        });
        receipt.value["errors"]
            .as_array_mut()
            .ok_or_else(NumeraError::runtime)?
            .push(json!(error));
        if ambiguous {
            receipt.value["generation_outcome"] = json!("unknown");
        }
        if !receipt.value["upstream_job"].is_null()
            && receipt.value["upstream_terminal"].is_null()
            && (receipt.value["generation_outcome"] != "completed"
                || (crate::error::array(&receipt.value["outputs"]).is_empty()
                    && !locked(&self.store)?.has_pending(id)?))
        {
            receipt.value["status"] = json!("running");
            if receipt.value["generation_outcome"] != "completed" {
                receipt.value["generation_outcome"] = json!("running");
            }
            receipt.value["warnings"].as_array_mut().ok_or_else(NumeraError::runtime)?.push(json!("Local waiting stopped; existing upstream job may still be running. Use get_job, never resubmit."));
        }
        locked(&self.store)?.update(&mut receipt)?;
        self.logger.log(
            "ERROR",
            "generation",
            "Request did not complete.",
            json!({"requestId":id,"error":error}),
        );
        Ok(receipt)
    }
}
fn deadline_error() -> NumeraError {
    NumeraError::new(
        "request_timeout",
        "Waiting stopped; upstream completion and billing may be unknown.",
    )
    .stage("network")
}
pub fn cancelled() -> NumeraError {
    NumeraError::new(
        "cancelled",
        "Waiting stopped; upstream completion and billing may be unknown.",
    )
    .stage("network")
}
pub async fn wait(cancel: &CancellationToken, millis: u64) -> Result<()> {
    tokio::select! {_=cancel.cancelled()=>Err(cancelled()),_=tokio::time::sleep(Duration::from_millis(millis))=>Ok(())}
}
pub fn validate_job(result: &Normalized, job: &adapters::types::Job) -> Result<()> {
    let matches = result.job.as_ref().map_or_else(
        || result.upstream_id.as_deref() == Some(job.id.as_str()),
        |j| j.id == job.id && j.kind == job.kind,
    );
    if !matches {
        return Err(NumeraError::new(
            "invalid_response",
            "Polled response does not match the existing job.",
        )
        .stage("response"));
    }
    Ok(())
}
