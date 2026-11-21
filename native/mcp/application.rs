use crate::{
    compat::{
        canonical_json::fingerprint,
        value::{JsString, Value, json},
    },
    config::{Config, ConfigReload, snapshot_credentials},
    error::{NumeraError, Result},
    files::{images, output},
    jobs::Store,
    logging::Logger,
    runtime_paths::RuntimePaths,
    services::{
        discovery::Discovery,
        finalization::{SharedStore, locked},
        generation::Generation,
        health,
        identity::connection_identity,
        jobs,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

struct Snapshots {
    reload: Option<ConfigReload>,
    generations: Vec<Arc<Generation>>,
    current: Arc<Generation>,
    owners: HashMap<String, Arc<Generation>>,
    discoveries: HashMap<u64, Arc<Discovery>>,
    generation_ids: HashMap<usize, u64>,
    next_id: u64,
}
pub struct Application {
    snapshots: Mutex<Snapshots>,
    pub store: SharedStore,
    pub logger: Arc<Logger>,
    pub stop: CancellationToken,
}
impl Application {
    pub fn new(
        config: Config,
        reload: Option<ConfigReload>,
        logger: Arc<Logger>,
        runtime: RuntimePaths,
        env: HashMap<String, String>,
        stop: CancellationToken,
    ) -> Result<Self> {
        let store = Arc::new(Mutex::new(Store::open(std::path::Path::new(
            config.value["stateDir"]
                .as_str()
                .ok_or_else(NumeraError::runtime)?,
        ))?));
        let generation = Arc::new(Generation {
            config: Arc::new(config),
            store: store.clone(),
            logger: logger.clone(),
            runtime,
            env: env.into(),
            active: Arc::new(Mutex::new(HashMap::new())),
        });
        let initial_ptr = Arc::as_ptr(&generation) as usize;
        Ok(Self {
            snapshots: Mutex::new(Snapshots {
                reload,
                generations: vec![generation.clone()],
                current: generation,
                owners: HashMap::new(),
                discoveries: HashMap::from([(1, Arc::new(Discovery::default()))]),
                generation_ids: HashMap::from([(initial_ptr, 1)]),
                next_id: 2,
            }),
            store,
            logger,
            stop,
        })
    }
    pub fn current(&self) -> Result<Arc<Generation>> {
        let mut snapshots = self.snapshots.lock().map_err(|_| NumeraError::runtime())?;
        if let Some(reload) = snapshots.reload.as_mut() {
            let config = reload.refresh(&self.logger, |_| true)?;
            if !Arc::ptr_eq(&config, &snapshots.current.config)
                && fingerprint(&config.value)? != fingerprint(&snapshots.current.config.value)?
            {
                let old = &snapshots.current;
                let generation = Arc::new(Generation {
                    config,
                    store: self.store.clone(),
                    logger: self.logger.clone(),
                    runtime: old.runtime.clone(),
                    env: old.env.clone(),
                    active: Arc::new(Mutex::new(HashMap::new())),
                });
                let id = snapshots.next_id;
                snapshots.next_id = snapshots.next_id.saturating_add(1);
                snapshots
                    .generation_ids
                    .insert(Arc::as_ptr(&generation) as usize, id);
                snapshots
                    .discoveries
                    .insert(id, Arc::new(Discovery::default()));
                snapshots.generations.push(generation.clone());
                snapshots.current = generation;
            }
            if snapshots.generations.len() >= 64 {
                let current = snapshots.current.clone();
                snapshots.generations.retain(|g| {
                    Arc::ptr_eq(g, &current)
                        || Arc::strong_count(g) > 1
                        || g.active.lock().is_ok_and(|a| !a.is_empty())
                });
                let live_ptrs: HashSet<usize> = snapshots
                    .generations
                    .iter()
                    .map(|g| Arc::as_ptr(g) as usize)
                    .collect();
                snapshots
                    .generation_ids
                    .retain(|ptr, _| live_ptrs.contains(ptr));
                let live_ids: HashSet<u64> = snapshots.generation_ids.values().copied().collect();
                snapshots.discoveries.retain(|id, _| live_ids.contains(id));
                if let Some(reload) = snapshots.reload.as_mut() {
                    reload.prune_unused();
                }
            }
        }
        Ok(snapshots.current.clone())
    }
    pub fn shutdown(&self) {
        self.stop.cancel();
        if let Ok(snapshots) = self.snapshots.lock() {
            for g in &snapshots.generations {
                if let Ok(active) = g.active.lock() {
                    for token in active.values() {
                        token.cancel();
                    }
                }
            }
        }
    }
    async fn owner(&self, id: &str, upstream: bool) -> Result<Arc<Generation>> {
        let (generations, preferred, current) = {
            let snapshots = self.snapshots.lock().map_err(|_| NumeraError::runtime())?;
            let active = snapshots
                .generations
                .iter()
                .find(|g| g.active.lock().is_ok_and(|a| a.contains_key(id)))
                .cloned();
            (
                snapshots.generations.clone(),
                snapshots.owners.get(id).cloned().or(active),
                snapshots.current.clone(),
            )
        };
        if !upstream {
            return Ok(preferred.unwrap_or(current));
        }
        let receipt = locked(&self.store)?.get(id)?;
        let identity = locked(&self.store)?.identity(id)?;
        let name = receipt.value["connection"]
            .as_str()
            .ok_or_else(NumeraError::runtime)?;
        let candidates = preferred.map_or(generations, |g| vec![g]);
        let mut matches = Vec::new();
        let mut routes = HashSet::new();
        for g in candidates {
            if let Ok((_, c)) = g.config.select(Some(name))
                && let Ok(pinned) = snapshot_credentials(&c, &g.env, &g.runtime).await
                && connection_identity(name, &pinned)? == identity
            {
                let mut route =
                    json!({"paths":c["paths"],"query":c["query"],"baseUrlMode":c["baseUrlMode"]});
                for key in ["proxy", "workflow"] {
                    if let Some(value) = c.get(key) {
                        route[key] = value.clone();
                    }
                }
                routes.insert(fingerprint(&route)?);
                matches.push(g);
            }
        }
        if routes.len() == 1
            && let Some(g) = matches.into_iter().next()
        {
            return Ok(g);
        }
        Err(NumeraError::new(
            "permission_denied",
            "Existing job connection identity is unavailable or ambiguous; no upstream request was sent.",
        ))
    }

    pub async fn call(&self, name: &str, args: Value, cancel: &CancellationToken) -> Result<Value> {
        self.call_snapshot(self.current()?, name, args, cancel)
            .await
    }
    pub async fn call_snapshot(
        &self,
        generation: Arc<Generation>,
        name: &str,
        args: Value,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let config = &generation.config;
        match name {
            "health_check" => {
                health::health(
                    config,
                    &generation.env,
                    &generation.runtime,
                    boolean(&args, "probe", false)?,
                    cancel,
                )
                .await
            }
            "list_connections" => Ok(
                json!({"connections":health::connections(config,&generation.env,&generation.runtime).await?}),
            ),
            "list_models" => {
                let (offset, limit) = pagination(&args)?;
                let discovery = {
                    let mut snapshots =
                        self.snapshots.lock().map_err(|_| NumeraError::runtime())?;
                    let ptr = Arc::as_ptr(&generation) as usize;
                    let id = if let Some(&id) = snapshots.generation_ids.get(&ptr) {
                        id
                    } else {
                        let id = snapshots.next_id;
                        snapshots.next_id = snapshots.next_id.saturating_add(1);
                        snapshots.generation_ids.insert(ptr, id);
                        id
                    };
                    snapshots
                        .discoveries
                        .entry(id)
                        .or_insert_with(|| Arc::new(Discovery::default()))
                        .clone()
                };
                let mut result = discovery
                    .list_shared(
                        config,
                        &generation.runtime,
                        &generation.env,
                        optional_string(&args, "connection")?,
                        boolean(&args, "refresh", false)?,
                        cancel,
                    )
                    .await?;
                let all = result["models"]
                    .as_array()
                    .ok_or_else(NumeraError::runtime)?;
                let next = if all.len() > offset + limit {
                    json!(offset + limit)
                } else {
                    Value::Null
                };
                result["models"] = json!(
                    all.iter()
                        .skip(offset)
                        .take(limit)
                        .cloned()
                        .collect::<Vec<_>>()
                );
                result["next_offset"] = next;
                Ok(result)
            }
            "get_model_capabilities" => {
                Discovery::model(
                    config,
                    &generation.runtime,
                    &generation.env,
                    optional_string(&args, "connection")?,
                    required_js_string(&args, "model")?,
                    cancel,
                )
                .await
            }
            "generate_media" => {
                let receipt = crate::services::media::run(&generation, args, cancel).await?;
                if !receipt.value["upstream_job"].is_null()
                    && !matches!(
                        receipt.status(),
                        "completed" | "partial" | "failed" | "cancelled"
                    )
                {
                    self.snapshots
                        .lock()
                        .map_err(|_| NumeraError::runtime())?
                        .owners
                        .insert(receipt.id()?.into(), generation);
                }
                Ok(receipt.value)
            }
            "get_media_output_info" => {
                let id = required_string(&args, "output_id")?;
                if uuid::Uuid::parse_str(id).is_err() {
                    return Err(invalid());
                }
                let record = locked(&self.store)?.output(id)?;
                let (mut metadata, _) = output::owned_media_from_record(&config.value, record)?;
                metadata["verified"] = json!(true);
                Ok(metadata)
            }
            "generate_image" | "edit_image" => {
                let operation = if name == "generate_image" {
                    "generate"
                } else {
                    "edit"
                };
                let receipt = if crate::services::fal_image::selected(&generation.config, &args) {
                    let media = crate::services::fal_image::to_media_request(&args, operation)?;
                    crate::services::media::run(&generation, media, cancel).await?
                } else {
                    generation.run(args, operation, cancel).await?
                };
                if !receipt.value["upstream_job"].is_null()
                    && !matches!(
                        receipt.status(),
                        "completed" | "partial" | "failed" | "cancelled"
                    )
                {
                    self.snapshots
                        .lock()
                        .map_err(|_| NumeraError::runtime())?
                        .owners
                        .insert(receipt.id()?.into(), generation);
                }
                Ok(receipt.value)
            }
            "get_job" => {
                let id = required_string(&args, "request_id")?;
                let refresh = boolean(&args, "refresh", false)?;
                let mut receipt = locked(&self.store)?.get(id)?;
                if receipt.value["operation"] == "media" {
                    let local = locked(&self.store)?.needs_local_recovery(id)?;
                    let upstream = refresh
                        && !local
                        && !receipt.value["upstream_job"].is_null()
                        && !matches!(
                            receipt.status(),
                            "completed" | "partial" | "failed" | "cancelled"
                        );
                    let owner = self.owner(id, upstream).await?;
                    let done =
                        crate::services::media_jobs::get(&owner, id, refresh, cancel).await?;
                    if matches!(
                        done.status(),
                        "completed" | "partial" | "failed" | "cancelled"
                    ) && let Ok(mut snapshots) = self.snapshots.lock()
                    {
                        snapshots.owners.remove(id);
                    }
                    return Ok(done.value);
                }
                let local = refresh && { locked(&self.store)?.needs_local_recovery(id)? };
                if local {
                    let owner = self.owner(id, false).await?;
                    receipt = jobs::recover_local(owner.as_ref(), id, cancel).await?;
                }
                let pending = { locked(&self.store)?.needs_local_recovery(id)? };
                let upstream = refresh
                    && !pending
                    && !receipt.value["upstream_job"].is_null()
                    && !matches!(receipt.status(), "completed" | "partial" | "failed");
                let owner = self.owner(id, upstream).await?;
                let done = jobs::get(owner.as_ref(), id, refresh, cancel, None).await?;
                if matches!(
                    done.status(),
                    "completed" | "partial" | "failed" | "cancelled"
                ) && let Ok(mut snapshots) = self.snapshots.lock()
                {
                    snapshots.owners.remove(id);
                }
                Ok(done.value)
            }
            "cancel_job" => {
                let id = required_string(&args, "request_id")?;
                let receipt = locked(&self.store)?.get(id)?;
                if receipt.value["operation"] == "media" {
                    crate::services::media_jobs::cancel_local(&generation, id)?;
                    let upstream = !receipt.value["upstream_job"].is_null()
                        && !matches!(
                            receipt.status(),
                            "completed" | "partial" | "failed" | "cancelled"
                        );
                    let owner = self.owner(id, upstream).await?;
                    let outcome = crate::services::media_jobs::cancel(&owner, id, cancel).await?;
                    if let Ok(mut snapshots) = self.snapshots.lock() {
                        snapshots.owners.remove(id);
                    }
                    return Ok(outcome);
                }
                locked(&self.store)?.cancel(id)?;
                let owner = self
                    .owner(id, receipt.value["upstream_job"]["kind"] == "comfyui")
                    .await?;
                let outcome = jobs::cancel(&owner, id, cancel).await?;
                if let Ok(mut snapshots) = self.snapshots.lock() {
                    snapshots.owners.remove(id);
                }
                Ok(outcome)
            }
            "list_jobs" => {
                let status = optional_string(&args, "status")?;
                if let Some(status) = status
                    && !matches!(
                        status,
                        "running"
                            | "outcome_unknown"
                            | "completed"
                            | "partial"
                            | "failed"
                            | "cancelled"
                    )
                {
                    return Err(invalid());
                }
                let (offset, limit) = pagination(&args)?;
                let all = locked(&self.store)?.list_jobs(status, offset as u32, limit as u32)?;
                Ok(json!({
                    "jobs": all.iter().take(limit).collect::<Vec<_>>(),
                    "next_offset": if all.len() > limit { json!(offset + limit) } else { Value::Null }
                }))
            }
            "list_outputs" => {
                let (offset, limit) = pagination(&args)?;
                let all = locked(&self.store)?.list_outputs(offset as u32, limit as u32)?;
                Ok(
                    json!({"outputs":all.iter().take(limit).collect::<Vec<_>>(),"next_offset":if all.len()>limit{json!(offset+limit)}else{Value::Null}}),
                )
            }
            "get_output_info" => {
                let id = required_string(&args, "output_id")?;
                if uuid::Uuid::parse_str(id).is_err() {
                    return Err(invalid());
                }
                let record = locked(&self.store)?.output(id)?;
                let (mut result, _) = output::owned_from_record(&config.value, record)?;
                result["verified"] = json!(true);
                Ok(result)
            }
            _ => Err(NumeraError::new("invalid_input", "Unknown tool name.")),
        }
    }

    pub fn original_media(&self, id: &str) -> Result<(Value, Vec<u8>)> {
        let generation = self.current()?;
        let record = locked(&self.store)?.output(id)?;
        output::owned_media_from_record(&generation.config.value, record)
    }
    pub fn original(&self, id: &str) -> Result<images::Image> {
        let generation = self.current()?;
        let record = locked(&self.store)?.output(id)?;
        Ok(output::owned_from_record(&generation.config.value, record)?.1)
    }
    pub fn preview(&self, id: &str) -> Result<Value> {
        let generation = self.current()?;
        let record = locked(&self.store)?.output(id)?;
        let image = output::owned_from_record(&generation.config.value, record)?.1;
        images::preview(&image, &generation.config.value)
    }
    pub fn return_mode(&self) -> Result<String> {
        Ok(self.current()?.config.value["returnMode"]
            .as_str()
            .unwrap_or("files")
            .into())
    }
}
fn invalid() -> NumeraError {
    NumeraError::new(
        "invalid_input",
        "Tool arguments do not match the public schema.",
    )
}
pub fn optional_string<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.checked_str().map_err(|_| invalid())?)),
        _ => Err(invalid()),
    }
}
pub fn required_js_string<'a>(args: &'a Value, key: &str) -> Result<&'a JsString> {
    match args.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Ok(value),
        _ => Err(invalid()),
    }
}
pub fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    optional_string(args, key)?
        .filter(|s| !s.is_empty())
        .ok_or_else(invalid)
}
pub fn boolean(args: &Value, key: &str, default: bool) -> Result<bool> {
    match args.get(key) {
        None => Ok(default),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(invalid()),
    }
}
fn pagination(args: &Value) -> Result<(usize, usize)> {
    let parse = |key: &str, default: u64, min: u64, max: u64| -> Result<usize> {
        let n = args
            .get(key)
            .map_or(Some(default as f64), Value::as_f64)
            .ok_or_else(invalid)?;
        if n.fract() != 0.0 || n < min as f64 || n > max as f64 {
            return Err(invalid());
        }
        Ok(n as usize)
    };
    Ok((parse("offset", 0, 0, 100000)?, parse("limit", 25, 1, 100)?))
}
