use super::{fixtures as f,host::{self,structured},provider::{Provider,Reply}};
use serde_json::json;
use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};

pub async fn run(){
    for(adapter,kind,status)in[("openai-responses","responses","failed"),("openai-responses","responses","cancelled"),("gemini-interactions","interactions","failed"),("openai-responses","responses","incomplete")]{host::scenario(10000,authoritative(adapter,kind,status)).await;}
    for mode in["in_progress","401","429","500","disconnect","malformed","wrong-id","identity"]{host::scenario(10000,uncertainty(mode)).await;}
    for mode in["confirmed","dispatched","false","missing","ambiguous","completed"]{host::scenario(10000,cancellation(mode)).await;}
}
fn connection(origin:&str,adapter:&str)->serde_json::Value{
    let mut value=json!({"adapter":adapter,"baseUrl":origin,"auth":{"type":"none"}});
    if adapter=="comfyui"{value["baseUrlMode"]=json!("origin");}
    else{value["defaultModel"]=json!("image-model");}
    value
}
async fn seed(config:&serde_json::Value,directory:&std::path::Path,kind:&str,state:&str,wrong:Option<&str>)->String{
    let identity=f::identity(config,"local",directory).await;let mut store=f::store(config);
    // The frozen lifecycle receipt deliberately says openai-images even when the selected
    // connection adapter is Responses/Interactions/Comfy; preserve that literal seed.
    let mut old=store.prepare(f::receipt("old","local","openai-images"),"hash",wrong.unwrap_or(&identity)).unwrap().1;
    assert!(store.admit("old","local",1,1).unwrap());
    old.value["status"]=numera_gen_mcp::compat::value::Value::from(state);
    old.value["generation_outcome"]=numera_gen_mcp::compat::value::Value::from(state);
    old.value["upstream_job"]=numera_gen_mcp::compat::value::Value::parse(&json!({"id":"job","kind":kind}).to_string()).unwrap();
    store.update(&mut old).unwrap();store.release("old",Some(&old)).unwrap();identity
}
async fn authoritative(adapter:&'static str,kind:&'static str,status:&'static str){
    let work=f::workspace();let directory=f::directory(&work,&format!("literal-terminal-{adapter}-{status}"));
    let gets=Arc::new(AtomicUsize::new(0));let posts=Arc::new(AtomicUsize::new(0));let g=gets.clone();let p=posts.clone();
    let wire=Provider::new(move|request,_|{let g=g.clone();let p=p.clone();async move{if request.method=="POST"{p.fetch_add(1,Ordering::SeqCst);}else{g.fetch_add(1,Ordering::SeqCst);}assert_eq!(request.method,"GET");assert_eq!(request.path,format!("/{kind}/job"));assert_eq!(request.header("authorization"),None);Ok(Reply::json(json!({"id":"job","status":status})))}}).await;
    let config=f::config(&directory,json!({"local":connection(&wire.origin,adapter)}));assert_defaults(&config);
    let identity=seed(&config,&directory,kind,"running",None).await;
    let mut store=f::store(&config);
    for reopen in 0..3{
        let mut host=f::start(&directory,&config,"terminal-test").await;
        for _ in 0..2{let result=host.call("get_job",json!({"request_id":"old","refresh":true})).await;assert_ne!(result["isError"],true,"{result}");let value=structured(&result);assert_eq!(value["status"],if status=="incomplete"{"failed"}else{status});assert_eq!(value["generation_outcome"],status);assert_eq!(value["upstream_job"],json!({"id":"job","kind":kind}));}
        f::admission(&mut store,&format!("fresh-{reopen}"),&identity,true);assert!(!store.prepare(f::receipt("old","local","openai-images"),"hash",&identity).unwrap().0);
        host.close().await.unwrap();drop(store);if reopen<2{store=f::store(&config);}else{break;}
    }
    assert_eq!(gets.load(Ordering::SeqCst),1);assert_eq!(posts.load(Ordering::SeqCst),0);wire.close().await;
}
fn assert_defaults(config:&serde_json::Value){assert_eq!(config["connections"]["local"]["requestTimeoutMs"],300000);assert_eq!(config["connections"]["local"]["discoveryTimeoutMs"],15000);}
async fn uncertainty(mode:&'static str){
    let work=f::workspace();let directory=f::directory(&work,&format!("literal-uncertainty-{mode}"));let gets=Arc::new(AtomicUsize::new(0));let posts=Arc::new(AtomicUsize::new(0));let g=gets.clone();let p=posts.clone();
    let wire=Provider::new(move|request,_|{let g=g.clone();let p=p.clone();async move{if request.method=="POST"{p.fetch_add(1,Ordering::SeqCst);}else{g.fetch_add(1,Ordering::SeqCst);}assert_eq!(request.method,"GET");assert_eq!(request.path,"/responses/job");assert_eq!(request.header("authorization"),None);
        Ok(match mode{"disconnect"=>Reply::disconnect(),"malformed"=>Reply::bytes(200,"",b"not-json".to_vec()),_=>Reply::status(if mode=="wrong-id"{json!({"id":"other","status":"failed"})}else{json!({"id":"job","status":"in_progress"})},mode.parse::<u16>().ok().filter(|s|[401,429,500].contains(s)).unwrap_or(200))})
    }}).await;
    let config=f::config(&directory,json!({"local":connection(&wire.origin,"openai-responses")}));assert_defaults(&config);
    let identity=seed(&config,&directory,"responses","running",if mode=="identity"{Some("other")}else{None}).await;
    let mut store=f::store(&config);let mut host=f::start(&directory,&config,"poll-control").await;
    host.call("get_job",json!({"request_id":"old","refresh":true})).await;
    let old=f::as_json(&store.get("old").unwrap());assert!(old.get("upstream_terminal").is_none());assert_eq!(old["status"],"running");f::admission(&mut store,"fresh",&identity,false);assert_eq!(posts.load(Ordering::SeqCst),0);assert_eq!(gets.load(Ordering::SeqCst),usize::from(mode!="identity"));host.close().await.unwrap();drop(store);wire.close().await;
}
async fn cancellation(mode:&'static str){
    let work=f::workspace();let directory=f::directory(&work,&format!("literal-cancellation-{mode}"));let cancellations=Arc::new(AtomicUsize::new(0));let generations=Arc::new(AtomicUsize::new(0));let c=cancellations.clone();let g=generations.clone();
    let wire=Provider::new(move|request,_|{let c=c.clone();let g=g.clone();async move{assert_eq!(request.header("authorization"),None);if request.method=="POST"{if request.path.ends_with("/cancel"){c.fetch_add(1,Ordering::SeqCst);assert_eq!(request.path,"/api/jobs/job/cancel");Ok(if mode=="ambiguous"{Reply::disconnect()}else{Reply::json(if mode=="missing"{json!({})}else{json!({"cancelled":mode!="false"})})})}else{g.fetch_add(1,Ordering::SeqCst);Ok(Reply::json(json!({})))}}else{assert_eq!(request.method,"GET");assert_eq!(request.path,"/api/jobs/job");Ok(Reply::json(json!({"id":"job","status":if mode=="confirmed"{"cancelled"}else{"in_progress"}})))}}}).await;
    let config=f::config(&directory,json!({"local":connection(&wire.origin,"comfyui")}));assert_defaults(&config);
    let identity=seed(&config,&directory,"comfyui",if mode=="completed"{"completed"}else{"running"},None).await;
    let mut host=f::start(&directory,&config,"cancel-contract").await;
    for _ in 0..2{let result=host.call("cancel_job",json!({"request_id":"old"})).await;if mode!="ambiguous"{let value=structured(&result);assert_eq!(value["refund_verified"],false);assert_eq!(value["upstream_cancelled"],match mode{"confirmed"=>json!(true),"missing"|"completed"=>json!(null),_=>json!(false)});}}
    host.close().await.unwrap();
    for n in 0..2{let mut store=f::store(&config);assert_eq!(f::as_json(&store.get("old").unwrap())["status"],match mode{"confirmed"=>"cancelled","completed"=>"completed",_=>"running"});f::admission(&mut store,&format!("fresh-{n}"),&identity,["confirmed","completed"].contains(&mode));}
    assert_eq!(generations.load(Ordering::SeqCst),0);assert_eq!(cancellations.load(Ordering::SeqCst),if mode=="completed"{0}else if mode=="confirmed"{1}else{2});wire.close().await;
}
