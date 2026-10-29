use super::{fixtures as f,host,provider::{Gate,Provider,Reply}};
use serde_json::json;
use std::{collections::HashSet,io::Write,sync::{Arc,atomic::{AtomicUsize,Ordering}}};

pub async fn run(){
    let(_,bytes)=f::image("png");
    for state in["failed","cancelled","incomplete","completed"]{
        host::scenario(30000,responses(state,&bytes)).await;
        writeln!(std::io::stderr().lock(),"{}",json!({"component":"native-races-acceptance","adapter":"openai-responses","state":state,"message":"Actual two-host race and restarts completed."})).unwrap();
    }
    for state in["cancelled","failed","completed"]{
        host::scenario(30000,comfy(state,&bytes)).await;
        writeln!(std::io::stderr().lock(),"{}",json!({"component":"native-races-acceptance","adapter":"comfyui","state":state,"message":"Actual two-host race and restarts completed."})).unwrap();
    }
}
async fn responses(state:&'static str,bytes:&[u8]){
    let work=f::workspace();let directory=f::directory(&work,&format!("stdio-responses-race-{state}"));let polled=Gate::new();let release=Gate::new();let entered=polled.clone();let held=release.clone();let posts=Arc::new(AtomicUsize::new(0));let polls=Arc::new(AtomicUsize::new(0));let p=posts.clone();let g=polls.clone();let encoded=base64::Engine::encode(&base64::engine::general_purpose::STANDARD,bytes);
    let wire=Provider::new(move|request,_|{let p=p.clone();let g=g.clone();let entered=entered.clone();let held=held.clone();let encoded=encoded.clone();async move{if request.method=="POST"{p.fetch_add(1,Ordering::SeqCst);return Ok(Reply::json(json!({"id":"owned","status":"in_progress"})));}if g.fetch_add(1,Ordering::SeqCst)==0{entered.release();host::bounded(5000,held.wait()).await;}let mut body=json!({"id":"owned","status":state});if["completed","incomplete"].contains(&state){body["output"]=json!([{"type":"image_generation_call","status":"completed","result":encoded}]);}Ok(Reply::json(body))}}).await;
    let config=f::config(&directory,json!({"local":{"adapter":"openai-responses","baseUrl":format!("{}/v1",wire.origin),"auth":{"type":"none"},"orchestrationModel":"text","defaultModel":"image","requestTimeoutMs":5000}}));let mut instances=HashSet::new();let mut hosts=host::pair(&directory,&config,&mut instances).await;
    {
        let original=hosts[0].call("generate_image",json!({"prompt":"draw","request_id":"race"}));tokio::pin!(original);
        tokio::select!{_ = host::bounded(5000,polled.wait())=>{},_ = &mut original=>panic!("original request ended before held poll")}
        let busy=hosts[1].job("race",true).await;assert_eq!(busy["status"],"running");assert_eq!(busy["upstream_job"]["id"],"owned");assert_eq!(polls.load(Ordering::SeqCst),1);release.release();original.await;
    }
    let persisted=hosts[0].job("race",true).await;let terminal=hosts[1].job("race",true).await;assert_eq!(persisted["generation_outcome"],state);assert_eq!(terminal["generation_outcome"],state);assert_eq!(persisted["outputs"],terminal["outputs"]);if["completed","incomplete"].contains(&state){f::saved(&terminal,bytes);}assert_eq!(posts.load(Ordering::SeqCst),1);assert_eq!(polls.load(Ordering::SeqCst),1);host::close_pair(&mut hosts).await;
    for _ in 0..2{let mut reopened=host::pair(&directory,&config,&mut instances).await;for owner in &reopened{assert_eq!(owner.job("race",true).await["generation_outcome"],state);}host::close_pair(&mut reopened).await;}
    assert_eq!(posts.load(Ordering::SeqCst),1);wire.close().await;
}
async fn comfy(state:&'static str,bytes:&[u8]){
    let work=f::workspace();let directory=f::directory(&work,&format!("stdio-comfy-race-{state}"));let observed=Gate::new();let release=Gate::new();let entered=observed.clone();let held=release.clone();let posts=Arc::new(AtomicUsize::new(0));let cancellations=Arc::new(AtomicUsize::new(0));let downloads=Arc::new(AtomicUsize::new(0));let p=posts.clone();let c=cancellations.clone();let d=downloads.clone();let original=bytes.to_vec();
    let wire=Provider::new(move|request,_|{let p=p.clone();let c=c.clone();let d=d.clone();let entered=entered.clone();let held=held.clone();let original=original.clone();async move{
        Ok(match request.path.as_str(){"/object_info"=>Reply::json(f::comfy_objects()),"/system_stats"=>Reply::json(json!({"devices":[{"type":"cuda"}]})),"/prompt"=>{p.fetch_add(1,Ordering::SeqCst);Reply::json(json!({"prompt_id":"owned"}))},"/history/owned"=>{if state!="completed"{entered.release();host::bounded(5000,held.wait()).await;Reply::json(json!({}))}else{Reply::json(f::comfy_history("final.png","owned"))}},"/api/jobs/owned/cancel"=>{c.fetch_add(1,Ordering::SeqCst);Reply::json(json!({"cancelled":true}))},"/api/jobs/owned"=>Reply::json(json!({"id":"owned","status":if state=="completed"{"cancelled"}else{state}})),path if path.starts_with("/view")=>{if d.fetch_add(1,Ordering::SeqCst)==0{entered.release();host::bounded(5000,held.wait()).await;}Reply::bytes(200,"image/png",original)},_=>Reply::status(json!({}),404)})
    }}).await;
    let config=f::config(&directory,json!({"local":{"adapter":"comfyui","baseUrl":wire.origin,"baseUrlMode":"origin","auth":{"type":"none"},"defaultModel":"workflow","requestTimeoutMs":5000,"workflow":f::comfy_workflow()}}));let mut instances=HashSet::new();let mut hosts=host::pair(&directory,&config,&mut instances).await;
    {
        let original=hosts[0].call("generate_image",json!({"prompt":"draw","request_id":"race"}));tokio::pin!(original);
        tokio::select!{_ = host::bounded(5000,observed.wait())=>{},_ = &mut original=>panic!("original request ended before controlled Comfy race")}
        let cancelled=hosts[1].call("cancel_job",json!({"request_id":"race"})).await;assert_ne!(cancelled["isError"],true);assert_eq!(cancelled["structuredContent"]["refund_verified"],false);assert_eq!(cancelled["structuredContent"]["upstream_cancelled"],state=="cancelled");
        // The source waits for local cancellation before allowing the held provider handler to drain.
        original.await;release.release();let current=hosts[1].job("race",false).await;assert_eq!(current["generation_outcome"],state);if state!="completed"{assert_eq!(current["status"],state);}
    }
    host::close_pair(&mut hosts).await;let mut output_id=None;
    for _ in 0..2{let mut reopened=host::pair(&directory,&config,&mut instances).await;for owner in &reopened{let current=owner.job("race",true).await;assert_eq!(current["generation_outcome"],state);assert_eq!(current["status"],state);if state=="completed"{f::saved(&current,bytes);if let Some(id)=&output_id{assert_eq!(&current["outputs"][0]["output_id"],id);}output_id=Some(current["outputs"][0]["output_id"].clone());}}host::close_pair(&mut reopened).await;}
    assert_eq!(posts.load(Ordering::SeqCst),1);assert_eq!(cancellations.load(Ordering::SeqCst),1);assert_eq!(downloads.load(Ordering::SeqCst),if state=="completed"{2}else{0});wire.close().await;
}
