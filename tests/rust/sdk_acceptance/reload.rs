use super::{fixtures as f,host::{self,Era,Host,Launch,structured},provider::{Gate,Provider,Reply}};
use serde_json::{Value,json};
use std::sync::{Arc,Mutex};

pub async fn run(){env_models().await;retained_handles().await;captured_generation().await;}
fn pairs(value:&Value)->Value{json!(value["models"].as_array().unwrap().iter().map(|m|json!({"id":m["id"],"name":m["name"]})).collect::<Vec<_>>())}
async fn env_models(){
    let work=f::workspace();let directory=f::directory(&work,"config-reload-env");
    let wire=Provider::new(|_,_|async{Ok(Reply::json(json!({"data":[]})))}).await;
    let file=directory.join(".env");let env_text=|id:&str,name:&str|format!("API_ENDPOINT={}/v1\nAPI_KEY=synthetic-test-key\nMODEL_1_ID={id}\nMODEL_1_NAME=\"{name}\"\nMODEL_1_SIZE=\n",wire.origin);
    std::fs::write(&file,env_text("first","First")).unwrap();let mut host=Host::connect(&directory,Launch::bridge(&directory,&file,"reload-fixture"),Era::Auto,false).await.unwrap();
    assert_eq!(pairs(&structured(&host.call("list_models",json!({})).await)),json!([{"id":"first","name":"First"}]));
    std::fs::write(&file,env_text("second","تصویر دوم")).unwrap();assert_eq!(pairs(&structured(&host.call("list_models",json!({})).await)),json!([{"id":"second","name":"تصویر دوم"}]));
    std::fs::write(&file,env_text("third","Third")+"MODEL_1_ENABLED=maybe\n").unwrap();
    for _ in 0..2{assert_eq!(pairs(&structured(&host.call("list_models",json!({})).await)),json!([{"id":"second","name":"تصویر دوم"}]));}
    assert!(wire.captured().is_empty());let logs=f::logs(&directory.join("logs"));assert_eq!(logs.matches("Saved configuration rejected").count(),1);assert!(!logs.contains("synthetic-test-key"));host.close().await.unwrap();wire.close().await;
}
async fn retained_handles(){
    let work=f::workspace();let directory=f::directory(&work,"config-reload-handles");let wire=Provider::new(|request,_|async{Ok(Reply::json(if request.method=="POST"{json!({"cancelled":true})}else if request.path.starts_with("/api/jobs/"){json!({"id":"original-handle","status":"cancelled"})}else{json!({})}))}).await;
    let first=f::config(&directory,json!({"local":{"adapter":"comfyui","baseUrl":wire.origin,"baseUrlMode":"origin","auth":{"type":"none"}}}));
    for(id,wrong)in[("existing",None),("unknown",Some("unavailable-account"))]{
        // Explicit seed-writer release is the original cross-process handoff, not same-Store proof.
        let identity=f::identity(&first,"local",&directory).await;let mut store=f::store(&first);let mut old=f::receipt(id,"local","comfyui");old.value["status"]=numera_gen_mcp::compat::value::Value::from("running");old.value["upstream_job"]=numera_gen_mcp::compat::value::Value::parse("{\"kind\":\"comfyui\",\"id\":\"original-handle\"}").unwrap();let saved=store.prepare(old,"fixture-fingerprint",wrong.unwrap_or(&identity)).unwrap().1;store.release(id,Some(&saved)).unwrap();
    }
    let file=f::write_config(&directory,&first);let mut launch=Launch::bridge(&directory,&file,"recovery-fixture");launch.args.extend(["--log-level".into(),"ERROR".into()]);let mut host=Host::connect(&directory,launch,Era::Auto,false).await.unwrap();
    let mut changed=first.clone();changed["connections"]["local"]["baseUrl"]=json!(format!("{}/changed",wire.origin));changed["connections"]["local"]["defaultModel"]=json!("new-model");f::write(&file,&changed);
    let poll=host.call("get_job",json!({"request_id":"existing","refresh":true})).await;assert_ne!(poll["isError"],true);
    let cancel=host.call("cancel_job",json!({"request_id":"existing"})).await;assert_eq!(cancel["structuredContent"]["upstream_requested"],true);assert_eq!(cancel["structuredContent"]["upstream_cancelled"],true);
    assert_eq!(wire.captured().iter().map(|r|r.path.as_str()).collect::<Vec<_>>(),vec!["/history/original-handle","/api/jobs/original-handle/cancel","/api/jobs/original-handle"]);
    let unknown=host.call("get_job",json!({"request_id":"unknown","refresh":true})).await;assert_eq!(unknown["isError"],true);assert_eq!(unknown["structuredContent"]["error"]["code"],"permission_denied");assert_eq!(wire.captured().len(),3);
    let mut changed=first.clone();changed["connections"]["local"]["paths"]=json!({"history":"changed-history"});f::write(&file,&changed);
    let ambiguous=host.call("get_job",json!({"request_id":"existing","refresh":true})).await;assert_eq!(ambiguous["isError"],true);assert_eq!(ambiguous["structuredContent"]["error"]["code"],"permission_denied");assert_eq!(wire.captured().len(),3);host.close().await.unwrap();wire.close().await;
}
async fn captured_generation(){
    let work=f::workspace();let directory=f::directory(&work,"config-reload-captured");let(row,png)=f::image("png");let submitted=Gate::new();let held=Gate::new();let entered=submitted.clone();let release=held.clone();let bodies=Arc::new(Mutex::new(Vec::new()));let capture=bodies.clone();
    let wire=Provider::new(move|request,_|{let entered=entered.clone();let release=release.clone();let row=row.clone();let capture=capture.clone();async move{capture.lock().unwrap().push(request.json());entered.release();host::bounded(8000,release.wait()).await;Ok(Reply::json(json!({"data":[{"b64_json":row["base64"]}]})))}}).await;
    let first=f::config(&directory,json!({"local":{"adapter":"openai-images","baseUrl":format!("{}/v1",wire.origin),"auth":{"type":"none"},"defaultModel":"old","configuredModels":[{"id":"old","name":"Old"}],"requestTimeoutMs":5000}}));
    let file=f::write_config(&directory,&first);let mut launch=Launch::bridge(&directory,&file,"snapshot-fixture");launch.args.extend(["--log-level".into(),"ERROR".into()]);let mut host=Host::connect(&directory,launch,Era::Auto,false).await.unwrap();
    {
    let pending=host.call_timeout("generate_image",json!({"prompt":"fixture","request_id":"captured-config"}),7000);tokio::pin!(pending);
    tokio::select!{_ = host::bounded(8000,submitted.wait())=>{},_ = &mut pending=>panic!("generation returned before fixture submission")}
    let mut changed=first.clone();changed["connections"]["local"]["baseUrl"]=json!(format!("{}/new/v1",wire.origin));changed["connections"]["local"]["defaultModel"]=json!("new");changed["connections"]["local"]["configuredModels"]=json!([{"id":"new","name":"New"}]);f::write(&file,&changed);
    let listed=host.call("list_models",json!({})).await;assert_eq!(pairs(&structured(&listed)),json!([{"id":"new","name":"New"}]));held.release();let result=pending.await;
    assert_eq!(result["structuredContent"]["status"],"completed");assert_eq!(result["structuredContent"]["requested_model"],"old");assert_eq!(result["structuredContent"]["outputs"][0]["bytes"],png.len());
    assert_eq!(*bodies.lock().unwrap(),vec![json!({"model":"old","prompt":"fixture","n":1})]);assert_eq!(wire.captured().iter().map(|r|(r.method.clone(),r.path.clone())).collect::<Vec<_>>(),vec![("POST".into(),"/v1/images/generations".into())]);f::saved(&structured(&result),&png);
    }
    host.close().await.unwrap();wire.close().await;
}
