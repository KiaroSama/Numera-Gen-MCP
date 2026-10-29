use super::{finalization::locked, generation::Generation, media_request::decode_data};
use crate::{compat::value::{Value,json},error::{NumeraError,Result},files::output,http::assets,jobs::Receipt,media::{self,Kind,fal::Artifact}};
use tokio_util::sync::CancellationToken;

fn check_reported_mime(reported:Option<&str>,verified:&str)->Result<()> {
    if let Some(mime)=reported {
        let mime=mime.split(';').next().unwrap_or("").trim();
        let matches=mime.eq_ignore_ascii_case(verified)
            || (verified=="audio/wav" && mime.eq_ignore_ascii_case("audio/x-wav"))
            || (verified=="image/jpeg" && mime.eq_ignore_ascii_case("image/jpg"));
        if !mime.is_empty() && !mime.eq_ignore_ascii_case("application/octet-stream") && !matches {
            return Err(NumeraError::new("invalid_response","Provider media MIME differs from the validated original bytes."));
        }
    }
    Ok(())
}

fn error(receipt:&mut Receipt,error:NumeraError)->Result<()> {
    receipt.value["errors"].as_array_mut().ok_or_else(NumeraError::storage)?.push(json!(error));Ok(())
}
pub async fn finalize(generation:&Generation,mut receipt:Receipt,artifacts:Option<Vec<Artifact>>,cancel:&CancellationToken,observe_cancel:bool)->Result<Receipt> {
    let id=receipt.id()?.to_owned();let config=&generation.config.value;
    let max=config["files"]["maxOutputBytes"].as_u64().unwrap_or(50*1024*1024) as usize;
    let aggregate=config["files"]["maxAggregateBytes"].as_i64().unwrap_or(100*1024*1024);
    let pixels=config["files"]["maxPixels"].as_u64().unwrap_or(64000000);
    locked(&generation.store)?.reserve(&id,aggregate,config["files"]["maxJournalBytes"].as_i64().unwrap_or(256*1024*1024))?;
    if !locked(&generation.store)?.owns(&id,Some(&receipt))? {return Err(NumeraError::storage());}
    receipt.value["status"]=json!("finalizing");receipt.value["storage_outcome"]=json!("writing");
    receipt.value["generation_outcome"]=if receipt.value["upstream_terminal"].is_null(){json!("completed")}else{receipt.value["upstream_terminal"].clone()};
    // Keep previous non-storage errors; only retry a retained local output, never a generation.
    if let Some(errors)=receipt.value["errors"].as_array_mut() {errors.retain(|v|!matches!(v["code"].as_str(),Some("output_file_error"|"storage_error"|"cancelled"|"request_timeout"|"outcome_unknown")));}
    if let Some(artifacts)=artifacts {
        if artifacts.is_empty()||artifacts.len()>64 {return Err(NumeraError::new("invalid_response","Media artifact count exceeds the shared durable journal bound."));}
        receipt.value["media_items"]=json!(artifacts.iter().map(|a|json!({"kind":a.kind.as_str(),"role":a.role,"reported_mime":a.reported_mime})).collect::<Vec<_>>());
        locked(&generation.store)?.update(&mut receipt)?;
        // Inline completion bytes are retained before observing a later wait cancellation.
        for (index,artifact) in artifacts.into_iter().enumerate() {
            let result=(||{
                if artifact.error.is_some(){return Err(NumeraError::new("invalid_response","Provider returned an invalid media artifact."));}
                if artifact.url.starts_with("data:") {
                    let bytes=decode_data(&artifact.url,artifact.kind,max)?;
                    let inspected=media::inspect(artifact.kind,&bytes,max,pixels)?;
                    let mime=artifact.url.split_once(',').and_then(|(head,_)|head.strip_prefix("data:")).and_then(|head|head.strip_suffix(";base64"));
                    if mime!=Some(inspected.mime.as_str()){return Err(NumeraError::new("invalid_response","Media data URI MIME differs from the validated bytes."));}
                    check_reported_mime(artifact.reported_mime.as_deref(),&inspected.mime)?;
                    locked(&generation.store)?.retain(&id,index as u32,&bytes,Some(&receipt))?;
                }else{
                    locked(&generation.store)?.retain_url(&id,index as u32,&artifact.url.into(),Some(&receipt))?;
                }
                Ok(())
            })();
            if let Err(reason)=result {error(&mut receipt,reason)?;}
        }
    }
    locked(&generation.store)?.update(&mut receipt)?;
    let items=locked(&generation.store)?.pending(&id)?;
    for item in items {
        let action=async {
            if cancel.is_cancelled() || (observe_cancel && locked(&generation.store)?.cancelled(&id)?) {return Err(super::generation::cancelled());}
            let descriptor=&receipt.value["media_items"][item.index as usize];
            let kind:Kind=serde_json::from_value(descriptor["kind"].to_serde()?).map_err(|_|NumeraError::storage())?;
            let role=descriptor["role"].as_str().ok_or_else(NumeraError::storage)?;
            let bytes=if let Some(bytes)=item.bytes {bytes}else{
                let bytes=assets::fetch(item.url.as_ref().ok_or_else(NumeraError::storage)?,config,max,cancel).await?;
                let inspected=media::inspect(kind,&bytes,max,pixels)?;
                check_reported_mime(descriptor["reported_mime"].as_str(),&inspected.mime)?;
                locked(&generation.store)?.retain(&id,item.index,&bytes,Some(&receipt))?;
                if cancel.is_cancelled() || (observe_cancel && locked(&generation.store)?.cancelled(&id)?) {return Err(super::generation::cancelled());}
                bytes
            };
            output::save_media(kind,role,&bytes,config,&mut *locked(&generation.store)?,output::Publication{
                id:&id,index:item.index,subdir:receipt.value["output_requirements"]["output_subdirectory"].as_str(),
                prefix:receipt.value["output_requirements"]["filename_prefix"].as_str(),deviations:&[],expected:Some(&receipt),stable:true})?;
            Ok::<(),NumeraError>(())
        }.await;
        if let Err(reason)=action {
            if !locked(&generation.store)?.owns(&id,Some(&receipt))? {return Err(reason);}
            error(&mut receipt,reason)?;
            if cancel.is_cancelled() || (observe_cancel && locked(&generation.store)?.cancelled(&id)?) {break;}
        }else{receipt.value["outputs"]=locked(&generation.store)?.get(&id)?.value["outputs"].clone();}
    }
    let pending=locked(&generation.store)?.has_pending(&id)?;
    let count=crate::error::array(&receipt.value["outputs"]).len();
    let errors=!crate::error::array(&receipt.value["errors"]).is_empty();
    receipt.value["status"]=json!(if count==0 && pending{"finalizing"}else if count==0{"failed"}else if pending||errors{"partial"}else{"completed"});
    receipt.value["storage_outcome"]=json!(if pending||errors{"partial_or_failed"}else{"completed"});
    locked(&generation.store)?.update(&mut receipt)?;
    generation.logger.log("INFO","media","Media originals processed.",json!({"count":count,"retained":pending,"status":receipt.status()}));
    Ok(receipt)
}
