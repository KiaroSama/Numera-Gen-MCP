use super::{fixtures as f,host::{self,structured},provider::{Provider,Reply},contracts::probe_stages};
use serde_json::{Value,json};
use std::{collections::{HashMap,HashSet},io::Write,sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}}};

pub async fn run(){let(row,_)=f::image("png");for adapter in["openai-responses","gemini-interactions"]{host::scenario(60000,terminal(adapter,row.clone())).await;}host::scenario(60000,comfy(row)).await;}
fn configuration(directory:&std::path::Path,connections:Value,default:&str)->Value{let mut value=f::config(directory,connections);value["defaultConnection"]=json!(default);value["maxConcurrentRequests"]=json!(32);value["files"]["maxAggregateBytes"]=json!(4096);value["files"]["maxJournalBytes"]=json!(1048576);value}
async fn terminal(adapter:&'static str,row:Value){
    let work=f::workspace();let directory=f::directory(&work,&format!("terminal-{adapter}"));let mut states=vec!["failed","cancelled","incomplete","completed"];if adapter=="gemini-interactions"{states.push("budget_exceeded");}let controls=["queued","in_progress","401","429","500","disconnect","malformed","wrong-id","identity"];let modes=states.iter().copied().chain(controls).collect::<Vec<_>>();
    let posts=Arc::new(Mutex::new(Vec::new()));let captured=posts.clone();let states_wire=states.clone();let row_wire=row.clone();
    let wire=Provider::new(move|request,_|{let captured=captured.clone();let states=states_wire.clone();let row=row_wire.clone();async move{
        let mode=request.path.split('/').nth(1).unwrap_or("").to_owned();let image=if adapter=="openai-responses"{json!({"output":[{"type":"image_generation_call","status":"completed","result":row["base64"]}]})}else{json!({"steps":[{"type":"model_output","content":[{"type":"image","data":row["base64"]}]}]})};
        if request.method=="POST"{captured.lock().unwrap().push(mode);let mut body=json!({"id":"probe","status":"completed"});body.as_object_mut().unwrap().extend(image.as_object().unwrap().clone());return Ok(Reply::json(body));}
        if mode=="disconnect"{return Ok(Reply::disconnect());}if mode=="malformed"{return Ok(Reply::bytes(200,"",b"not-json".to_vec()));}
        let mut body=json!({"id":if mode=="wrong-id"{"other"}else{mode.as_str()},"status":if states.contains(&mode.as_str()){mode.as_str()}else if mode=="queued"{"queued"}else{"in_progress"}});
        if["completed","incomplete"].contains(&mode.as_str()){body.as_object_mut().unwrap().extend(image.as_object().unwrap().clone());}
        Ok(Reply::status(body,mode.parse::<u16>().ok().filter(|s|[401,429,500].contains(s)).unwrap_or(200)))
    }}).await;
    let mut connections=serde_json::Map::new();for mode in &modes{connections.insert((*mode).into(),json!({"adapter":adapter,"baseUrl":format!("{}/{mode}/v1",wire.origin),"auth":{"type":"none"},"orchestrationModel":"text","defaultModel":"image","requestTimeoutMs":700,"maxConcurrentRequests":1}));}
    // Object.keys in the original selects numeric401 before insertion-ordered named modes.
    let config=configuration(&directory,Value::Object(connections),"401");let kind=if adapter=="openai-responses"{"responses"}else{"interactions"};
    for mode in &modes{f::seed(&config,&directory,mode,mode,kind,mode,"running",mode,if *mode=="identity"{Some("other-account")}else{None},(32,1)).await;}
    let bytes=base64::Engine::decode(&base64::engine::general_purpose::STANDARD,row["base64"].as_str().unwrap()).unwrap();let mut output_ids=HashMap::<String,Value>::new();let mut instances=HashSet::new();
    for round in 0..3{let mut hosts=host::pair(&directory,&config,&mut instances).await;for mode in &modes{
        let(a,b)=tokio::join!(hosts[0].call("get_job",json!({"request_id":mode,"refresh":true})),hosts[1].call("get_job",json!({"request_id":mode,"refresh":true})));let _=(a,b);
        for owner in &hosts{let current=owner.job(mode,false).await;if states.contains(mode){let state=if *mode=="budget_exceeded"{"failed"}else{*mode};assert_eq!(current["generation_outcome"],state);assert_eq!(current["status"],match *mode{"completed"=>"completed","incomplete"=>"partial","cancelled"=>"cancelled",_=>"failed"});
            if["completed","incomplete"].contains(mode){f::saved(&current,&bytes);if let Some(id)=output_ids.get(*mode){assert_eq!(&current["outputs"][0]["output_id"],id);}else{output_ids.insert((*mode).into(),current["outputs"][0]["output_id"].clone());}}
        }else{assert_eq!(current["status"],"running");assert!(current.get("upstream_terminal").is_none());}assert_eq!(current["upstream_job"],json!({"id":mode,"kind":kind}));}
        let before=posts.lock().unwrap().len();let before_stderr=hosts[1].text();let started=std::time::Instant::now();let probe=hosts[1].call("generate_image",json!({"connection":mode,"prompt":"admission probe","request_id":format!("probe-{mode}-{round}")})).await;
        let diagnostic=json!({"adapter":adapter,"mode":mode,"round":round,"is_error":probe["isError"],"status":probe["structuredContent"]["status"],"error":probe["structuredContent"]["error"]["code"],"elapsed_ms":started.elapsed().as_millis() as u64,"observed_posts":posts.lock().unwrap().len()-before,"native":probe_stages(&before_stderr,&hosts[1].text())});
        assert_eq!(posts.lock().unwrap().len()-before,usize::from(states.contains(mode)),"{diagnostic}");if states.contains(mode){assert_ne!(probe["isError"],true,"{diagnostic}");}
        let replay=hosts[0].call("generate_image",json!({"connection":mode,"prompt":"different fingerprint","request_id":mode})).await;assert_eq!(replay["isError"],true);assert!(replay.to_string().contains("request_id_conflict"));assert_eq!(posts.lock().unwrap().len()-before,usize::from(states.contains(mode)));
    }host::close_pair(&mut hosts).await;
        writeln!(std::io::stderr().lock(),"{}",json!({"component":"native-terminal-acceptance","adapter":adapter,"round":round,"message":"Terminal and polling control round completed.","cases":modes.len()})).unwrap();
    }
    assert_eq!(instances.len(),6);assert_eq!(posts.lock().unwrap().len(),states.len()*3);wire.close().await;
}
async fn comfy(row:Value){
    let work=f::workspace();let directory=f::directory(&work,"terminal-comfy-cancel");let modes=["cancelled","failed","dispatched","false","missing","ambiguous","wrong-id","completed"];let cancellations=Arc::new(AtomicUsize::new(0));let generations=Arc::new(AtomicUsize::new(0));let c=cancellations.clone();let g=generations.clone();let original=row.clone();
    let wire=Provider::new(move|request,_|{let c=c.clone();let g=g.clone();let row=original.clone();async move{
        let mode=request.path.split("fixture=").nth(1).unwrap_or("").split('&').next().unwrap_or("");let path=request.path.split('?').next().unwrap();
        if request.method=="POST"{if path.ends_with("/cancel"){c.fetch_add(1,Ordering::SeqCst);return Ok(if mode=="ambiguous"{Reply::disconnect()}else{Reply::json(if mode=="missing"{json!({})}else{json!({"cancelled":mode!="false"})})});}g.fetch_add(1,Ordering::SeqCst);return Ok(Reply::json(json!({"prompt_id":"probe"})));}
        Ok(if path.contains("/object_info"){Reply::json(f::comfy_objects())}else if path.contains("/system_stats"){Reply::json(json!({"devices":[{"type":"cuda"}]}))}else if path.contains("/history/probe"){Reply::json(f::comfy_history("probe.png","probe"))}else if path.contains("/view"){Reply::bytes(200,"image/png",base64::Engine::decode(&base64::engine::general_purpose::STANDARD,row["base64"].as_str().unwrap()).unwrap())}else if path.contains("/history/"){Reply::json(json!({}))}else{Reply::json(json!({"id":if mode=="wrong-id"{"other"}else{mode},"status":if["cancelled","failed"].contains(&mode){mode}else{"in_progress"}}))})
    }}).await;
    let mut connections=serde_json::Map::new();for mode in modes{connections.insert(mode.into(),json!({"adapter":"comfyui","baseUrl":wire.origin,"baseUrlMode":"origin","auth":{"type":"none"},"defaultModel":"workflow","requestTimeoutMs":700,"workflow":f::comfy_workflow(),"query":{"fixture":mode}}));}
    let config=configuration(&directory,Value::Object(connections),"cancelled");for mode in modes{f::seed(&config,&directory,mode,mode,"comfyui",mode,"running",mode,None,(32,1)).await;}
    {
        let mut store=f::store(&config);
        assert!(store.claim("completed",None,false).unwrap());
        let mut done=store.get("completed").unwrap();
        done.value["status"]=numera_gen_mcp::compat::value::Value::from("completed");
        done.value["generation_outcome"]=numera_gen_mcp::compat::value::Value::from("completed");
        store.update(&mut done).unwrap();store.release("completed",Some(&done)).unwrap();
    }
    let mut instances=HashSet::new();for round in 0..3{let mut hosts=host::pair(&directory,&config,&mut instances).await;for mode in modes{let result=hosts[0].call("cancel_job",json!({"request_id":mode})).await;if !["ambiguous","wrong-id"].contains(&mode){assert_ne!(result["isError"],true);let value=structured(&result);assert_eq!(value["refund_verified"],false);assert_eq!(value["upstream_cancelled"],if mode=="cancelled"{json!(true)}else if["missing","completed"].contains(&mode)||(mode=="failed"&&round>0){json!(null)}else{json!(false)});}else{assert_eq!(result["isError"],true);}
        for owner in &hosts{assert_eq!(owner.job(mode,true).await["status"],if["cancelled","failed","completed"].contains(&mode){mode}else{"running"});}
        let before=generations.load(Ordering::SeqCst);let probe=hosts[1].call("generate_image",json!({"connection":mode,"prompt":"capacity","request_id":format!("capacity-{mode}-{round}")})).await;if["cancelled","failed","completed"].contains(&mode){assert_ne!(probe["isError"],true,"{probe}");}assert_eq!(generations.load(Ordering::SeqCst)-before,usize::from(["cancelled","failed","completed"].contains(&mode)));
    }host::close_pair(&mut hosts).await;
        writeln!(std::io::stderr().lock(),"{}",json!({"component":"native-terminal-acceptance","adapter":"comfyui","round":round,"message":"Cancellation acknowledgement round completed.","cases":modes.len()})).unwrap();
    }
    assert_eq!(instances.len(),6);assert_eq!(generations.load(Ordering::SeqCst),9);assert_eq!(cancellations.load(Ordering::SeqCst),2+5*3);wire.close().await;
}
