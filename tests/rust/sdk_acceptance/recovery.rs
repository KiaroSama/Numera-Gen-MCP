use super::{fixtures as f,host::{self,Host,Era,Launch,structured},provider::{Provider,Reply}};
use rmcp::model::ReadResourceRequestParams;
use serde_json::{Value,json};

pub async fn run(){
    let work=f::workspace();let directory=f::directory(&work,"recovery-controls");let(image,bytes)=f::first_image();let original=bytes.clone();
    let wire=Provider::new(move|request,_|{let original=original.clone();async move{
        let parts=request.path.split('/').filter(|p|!p.is_empty()).collect::<Vec<_>>();let adapter=parts.first().copied().unwrap_or("");let mode=parts.get(1).copied().unwrap_or("");
        if request.method=="POST"{return Ok(Reply::status(json!({"error":"unexpected-submission"}),500));}
        if mode=="disconnect"{return Ok(Reply::disconnect());}if mode=="malformed"{return Ok(Reply::bytes(200,"application/json",b"not-json".to_vec()));}
        if["401","429","500"].contains(&mode){return Ok(Reply::status(json!({}),mode.parse().unwrap()));}
        let mut value=json!({"id":if mode=="wrong-id"{"another-job".into()}else{format!("original-{mode}")},"status":if mode=="wrong-id"{"completed"}else{mode}});
        if["incomplete","completed","wrong-id"].contains(&mode){let encoded=base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&original);if adapter=="responses"{value["output"]=json!([{"type":"image_generation_call","status":"completed","result":encoded}]);}else{value["steps"]=json!([{"type":"model_output","content":[{"type":"image","data":encoded}]}]);}}
        Ok(Reply::json(value))
    }}).await;
    let states=["failed","cancelled","incomplete","completed","budget_exceeded"];let controls=["queued","in_progress","401","429","500","disconnect","malformed","wrong-id"];
    let mut connections=serde_json::Map::new();let mut names=Vec::new();
    for adapter in["responses","interactions"]{for mode in states.iter().copied().filter(|mode|adapter=="interactions"||*mode!="budget_exceeded").chain(controls){let name=format!("{adapter}-{mode}");connections.insert(name.clone(),json!({"adapter":if adapter=="responses"{"openai-responses"}else{"gemini-interactions"},"baseUrl":format!("{}/{adapter}/{mode}",wire.origin),"auth":{"type":"none"},"defaultModel":"opaque/image","orchestrationModel":"opaque/orchestrator","requestTimeoutMs":3000,"discoveryTimeoutMs":2000}));names.push((name,adapter,mode));}}
    let mut config=f::config(&directory,Value::Object(connections));config["defaultConnection"]=json!(names[0].0);config["logging"]["level"]=json!("ERROR");config["files"]["maxAggregateBytes"]=json!(1048576);config["files"]["maxJournalBytes"]=json!(33554432);
    let file=f::write_config(&directory,&config);
    for era in[Era::Legacy,Era::Auto]{let mut host=Host::connect(&directory,Launch::bridge(&directory,&file,"native-differential-host"),era,false).await.unwrap();assert_eq!(serde_json::to_value(host.client().peer_info().unwrap().as_ref()).unwrap()["protocolVersion"],"2025-11-25");assert_eq!(host::bounded(5000,host.client().list_tools(None)).await.unwrap().tools.len(),12);host.close().await.unwrap();}
    assert_eq!(Host::connect(&directory,Launch::bridge(&directory,&file,"native-differential-host"),Era::Pin,false).await.err().unwrap(),"ERA_NEGOTIATION_FAILED");
    assert_eq!(Host::connect(&directory,Launch::bridge(&directory,&file,"native-differential-host"),Era::Legacy,true).await.err().unwrap(),"HANDSHAKE_CANCELLED_AND_CHILD_REAPED");
    for(name,adapter,mode)in &names{
        let identity=f::identity(&config,name,&directory).await;let mut store=f::store(&config);let id=format!("native-{name}");let mut receipt=f::receipt(&id,name,if *adapter=="responses"{"openai-responses"}else{"gemini-interactions"});receipt.value["requested_model"]=numera_gen_mcp::compat::value::Value::from("opaque/image");receipt.value["another_submission_may_charge"]=numera_gen_mcp::compat::value::Value::from(true);receipt.value["output_requirements"]=numera_gen_mcp::compat::value::Value::parse("{\"count\":1}").unwrap();let mut receipt=store.prepare(receipt,&id,&identity).unwrap().1;assert!(store.admit(&id,name,64,64).unwrap());receipt.value["status"]=numera_gen_mcp::compat::value::Value::from("running");receipt.value["generation_outcome"]=numera_gen_mcp::compat::value::Value::from("running");receipt.value["upstream_job"]=numera_gen_mcp::compat::value::Value::parse(&json!({"id":format!("original-{mode}"),"kind":adapter}).to_string()).unwrap();store.update(&mut receipt).unwrap();store.release(&id,Some(&receipt)).unwrap();
    }
    let mut pair=[f::start(&directory,&config,"recovery-host-0").await,f::start(&directory,&config,"recovery-host-1").await];
    let templates=host::bounded(6000,pair[0].client().list_resource_templates(None)).await.unwrap();
    assert_eq!(templates.resource_templates.len(),2);
    assert_eq!(templates.resource_templates[0].uri_template,"numera-image://outputs/{id}");
    assert_eq!(templates.resource_templates[1].uri_template,"numera-gen://outputs/{id}");
    assert_eq!(structured(&pair[0].call("health_check",json!({"probe":false})).await)["generation_verified"],false);
    assert_eq!(structured(&pair[0].call("list_models",json!({"connection":"responses-500","refresh":true})).await)["complete"],false);
    assert_eq!(structured(&pair[0].call("get_model_capabilities",json!({"connection":"responses-completed","model":"opaque/image"})).await)["account_availability"],"unknown");
    for args in[json!({"limit":0}),json!({"offset":-1}),json!({"limit":"25"})]{assert_eq!(pair[0].call("list_outputs",args).await["isError"],true);}
    for(name,_,mode)in &names{
        let id=format!("native-{name}");let receipt=pair[0].job(&id,true).await;
        if states.contains(mode){if *mode=="completed"{assert_eq!(receipt["status"],"completed");assert_eq!(receipt["outputs"].as_array().unwrap().len(),1);}else{assert_eq!(receipt["upstream_terminal"],if *mode=="budget_exceeded"{"failed"}else{*mode});assert_eq!(receipt["status"],match *mode{"incomplete"=>"partial","cancelled"=>"cancelled",_=>"failed"});if *mode=="incomplete"{assert_eq!(receipt["outputs"].as_array().unwrap().len(),1);}}
            for output in receipt["outputs"].as_array().unwrap(){assert_eq!(output["sha256"],image["metadata"]["sha256"]);assert_eq!(std::fs::read(output["path"].as_str().unwrap()).unwrap(),bytes);let oid=output["output_id"].as_str().unwrap();assert_eq!(structured(&pair[1].call("get_output_info",json!({"output_id":oid})).await)["verified"],true);let preview=pair[1].call("get_output_info",json!({"output_id":oid,"preview":true})).await;assert!(preview["content"].as_array().unwrap().iter().any(|i|i["type"]=="image"&&i["mimeType"]=="image/png"));let resource=host::bounded(6000,pair[1].client().read_resource(ReadResourceRequestParams::new(format!("numera-image://outputs/{oid}")))).await.unwrap();let value=serde_json::to_value(resource).unwrap();assert_eq!(value["contents"][0]["blob"],base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&bytes));}
        }else{assert_eq!(receipt["status"],"running");assert!(receipt.get("upstream_terminal").is_none());assert_eq!(receipt["outputs"],json!([]));if !["queued","in_progress"].contains(mode){assert!(!receipt["errors"].as_array().unwrap().is_empty());}}
        assert_eq!(pair[1].job(&id,false).await,receipt);
    }
    for restart in 0..2{host::close_pair(&mut pair).await;pair=[f::start(&directory,&config,"recovery-host-0").await,f::start(&directory,&config,"recovery-host-1").await];for(name,_,mode)in &names{let receipt=pair[restart%2].job(&format!("native-{name}"),false).await;if states.contains(mode)&&*mode!="completed"{assert_eq!(receipt["upstream_terminal"],if *mode=="budget_exceeded"{"failed"}else{*mode});}if["completed","incomplete"].contains(mode){f::saved(&receipt,&bytes);}}}
    host::close_pair(&mut pair).await;assert_eq!(wire.posts(),0,"recovery and restart never submit paid work");wire.close().await;
}
