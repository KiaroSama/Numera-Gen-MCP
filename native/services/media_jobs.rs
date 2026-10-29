use super::{finalization::locked,generation::Generation,identity::connection_identity,lease::Lease,media_finalization,media_wire};
use crate::{compat::value::{Value,json},config::snapshot_credentials,error::{NumeraError,Result},http::api::Body,jobs::Receipt,media::fal};
use reqwest::Method;
use tokio_util::sync::CancellationToken;

pub async fn get(generation:&Generation,id:&str,refresh:bool,host_cancel:&CancellationToken)->Result<Receipt> {
    let receipt=locked(&generation.store)?.get(id)?;
    let timeout=receipt.value["connection"].as_str().and_then(|name|generation.config.value["connections"].get(name))
        .and_then(|connection|connection["requestTimeoutMs"].as_u64()).unwrap_or(300000);
    let deadline=tokio::time::Instant::now()+std::time::Duration::from_millis(timeout);
    get_until(generation,id,refresh,host_cancel,deadline).await
}

pub async fn get_until(generation:&Generation,id:&str,refresh:bool,host_cancel:&CancellationToken,deadline:tokio::time::Instant)->Result<Receipt> {
    if !refresh {return locked(&generation.store)?.get(id);}
    locked(&generation.store)?.queued()?;
    let old=locked(&generation.store)?.get(id)?;
    let local=locked(&generation.store)?.needs_local_recovery(id)?;
    if !local && (old.value["upstream_job"].is_null() || matches!(old.status(),"completed"|"partial"|"failed"|"cancelled")) {return Ok(old);}
    if tokio::time::Instant::now()>=deadline || host_cancel.is_cancelled() {return Ok(old);}
    let mut deadline=deadline;
    let pinned=if local {None}else{
        let name=old.value["connection"].as_str().ok_or_else(NumeraError::storage)?;
        let (_,connection)=generation.config.select(Some(name))?;
        deadline=deadline.min(tokio::time::Instant::now()+std::time::Duration::from_millis(connection["requestTimeoutMs"].as_u64().unwrap_or(300000)));
        let pinned=tokio::time::timeout_at(deadline,snapshot_credentials(&connection,&generation.env,&generation.runtime)).await.map_err(|_|media_wire::deadline_error())??;
        if connection_identity(name,&pinned)?!=locked(&generation.store)?.identity(id)? {
            return Err(NumeraError::new("permission_denied","Existing media job destination/account changed; no recovery request was sent."));
        }
        Some(pinned)
    };
    if !locked(&generation.store)?.claim(id,None,false)? {return locked(&generation.store)?.get(id);}
    let (mut receipt,cancelled_before)=match (||->Result<_>{let store=locked(&generation.store)?;Ok((store.get(id)?,store.cancelled(id)?))})() {
        Ok(value)=>value,
        Err(error)=>{locked(&generation.store)?.release(id,None)?;return Err(error);}
    };
    let cancel=host_cancel.child_token();
    // A new explicit refresh after cancellation may recover the original completed handle. A
    // cancellation raised while this refresh is waiting must interrupt it across process owners.
    let lease=Lease::start(generation.store.clone(),receipt.clone(),cancel.clone(),!local&&!cancelled_before);
    let action=async {
        if local {
            return tokio::time::timeout_at(deadline,media_finalization::finalize(generation,receipt.clone(),None,&cancel,false)).await.map_err(|_|media_wire::deadline_error())?;
        }
        let pinned=pinned.as_ref().ok_or_else(NumeraError::runtime)?;
        let connection=pinned.value();let headers=pinned.headers();let handle=media_wire::stored(&receipt)?;
        let status_path=media_wire::tracking(connection,&handle,&handle.status_url,"status")?;
        let (_,status)=media_wire::request_until(connection,&headers,&status_path,Method::GET,Body::Empty,&cancel,false,deadline).await?;
        if status["request_id"].as_str()!=Some(handle.id.as_str()) {
            return Err(NumeraError::new("invalid_response","Queue status belongs to another request."));
        }
        match fal::status(&status)? {
            fal::Status::InQueue|fal::Status::InProgress=>{
                receipt.value["status"]=json!("running");receipt.value["generation_outcome"]=json!("running");
                locked(&generation.store)?.update(&mut receipt)?;Ok(receipt.clone())
            }
            fal::Status::Completed{failed:true,error_type}=>{
                let state=if error_type.as_deref()==Some("client_cancelled"){"cancelled"}else{"failed"};
                locked(&generation.store)?.terminate(id,state,&json!(NumeraError::new("provider_rejection","Existing media generation terminated without a successful result.").stage("generation")),Some(&receipt),Some(&receipt.value["upstream_job"]))
            }
            fal::Status::Completed{failed:false,..}=>{
                let result_path=media_wire::tracking(connection,&handle,&handle.response_url,"result")?;
                let (_,result)=media_wire::request_until(connection,&headers,&result_path,Method::GET,Body::Empty,&cancel,false,deadline).await?;
                let artifacts=fal::outputs(&handle.endpoint,&result)?;
                // Finalization journals already received inline originals before observing cancel.
                let publishing=cancel.child_token();
                if !cancelled_before && locked(&generation.store)?.cancelled(id)? {publishing.cancel();}
                tokio::time::timeout_at(deadline,media_finalization::finalize(generation,receipt.clone(),Some(artifacts),&publishing,!cancelled_before)).await.map_err(|_|media_wire::deadline_error())?
            }
        }
    }.await;
    let result=(||->Result<Receipt>{match action {Ok(value)=>Ok(value),Err(error)=>{
        let (current,retained_owned)={
            let store=locked(&generation.store)?;
            (store.get(id)?,store.has_pending(id)? && store.owns(id,Some(&receipt))?)
        };
        if current.value["generation_outcome"]=="completed" && retained_owned {
            let mut current=current;
            let has_outputs=!crate::error::array(&current.value["outputs"]).is_empty();
            current.value["status"]=json!(if has_outputs{"partial"}else{"finalizing"});
            current.value["storage_outcome"]=json!("partial_or_failed");
            current.value["errors"].as_array_mut().ok_or_else(NumeraError::storage)?.push(json!(error));
            locked(&generation.store)?.update(&mut current)?;Ok(current)
        }else{generation.record_error(id,receipt.clone(),error,cancel.is_cancelled())}
    }}})();
    lease.close().await;
    locked(&generation.store)?.release(id,Some(&receipt))?;
    result
}

pub fn cancel_local(generation:&Generation,id:&str)->Result<()> {
    locked(&generation.store)?.cancel(id)?;
    if let Ok(active)=generation.active.lock() && let Some(signal)=active.get(id){signal.cancel();}
    Ok(())
}

pub async fn cancel(generation:&Generation,id:&str,host_cancel:&CancellationToken)->Result<Value> {
    // Also enforce the local stop for callers other than the MCP application.
    cancel_local(generation,id)?;
    let receipt=locked(&generation.store)?.get(id)?;
    if receipt.value["upstream_job"].is_null() || matches!(receipt.status(),"completed"|"partial"|"failed"|"cancelled") {
        return Ok(json!({"request_id":id,"local_wait_cancelled":true,"upstream_cancellation":"not_requested","refund_verified":false}));
    }
    let name=receipt.value["connection"].as_str().ok_or_else(NumeraError::storage)?;
    let (_,connection)=generation.config.select(Some(name))?;
    let deadline=tokio::time::Instant::now()+std::time::Duration::from_millis(connection["requestTimeoutMs"].as_u64().unwrap_or(300000));
    let pinned=tokio::time::timeout_at(deadline,snapshot_credentials(&connection,&generation.env,&generation.runtime)).await.map_err(|_|media_wire::deadline_error())??;
    if connection_identity(name,&pinned)?!=locked(&generation.store)?.identity(id)? {return Err(NumeraError::new("permission_denied","Existing media cancellation destination/account changed."));}
    let handle=media_wire::stored(&receipt)?;
    let path=media_wire::tracking(pinned.value(),&handle,&handle.cancel_url,"cancel")?;
    let (status,value)=media_wire::request_until(pinned.value(),&pinned.headers(),&path,Method::PUT,Body::Empty,host_cancel,true,deadline).await?;
    let outcome=fal::cancel(&value,status)?;
    if outcome==fal::Cancel::Removed {
        // A completed response/journal won the race if it is already present. Otherwise the
        // upstream queued-removal confirmation may safely fence the old status waiter.
        let completed={let store=locked(&generation.store)?;store.has_pending(id)?||store.get(id)?.value["generation_outcome"]=="completed"};
        if !completed {
            locked(&generation.store)?.terminate(id,"cancelled",&json!(NumeraError::new("cancelled","Provider confirmed queued request removal; refund is not established.")),None,Some(&receipt.value["upstream_job"]))?;
        }
    }
    Ok(json!({"request_id":id,"local_wait_cancelled":true,"upstream_cancellation":match outcome{fal::Cancel::Removed=>"confirmed_queued_removal",fal::Cancel::Requested=>"requested",fal::Cancel::AlreadyCompleted=>"already_completed",fal::Cancel::NotFound=>"unknown"},"refund_verified":false}))
}
