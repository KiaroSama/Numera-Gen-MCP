use super::{fixtures as f, host::{self, Era, Host, Launch, structured}, provider::{Provider, Reply}};
use rmcp::model::ReadResourceRequestParams;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

pub async fn tools() {
    let work = f::workspace(); let directory = f::directory(&work, "tools-acceptance");
    let (original, bytes) = f::image("png");
    let posts = Arc::new(AtomicUsize::new(0)); let count = posts.clone();
    let wire = Provider::new(move |request, _| {
        let count = count.clone(); let original = original.clone();
        async move {
            Ok(Reply::json(if request.method == "POST" { count.fetch_add(1, Ordering::SeqCst); json!({"data":[{"b64_json":original["base64"]}]}) }
                else { json!({"data":[{"id":"image-model"}]}) }))
        }
    }).await;
    let config = f::config(&directory, json!({"local":{
        "adapter":"openai-images","baseUrl":format!("{}/v1",wire.origin),"auth":{"type":"none"},"defaultModel":"image-model",
        "edit":{"mode":"multipart","encoding":"image","maxReferences":1,"masks":true,"maskPolarity":"transparent-edit"}
    }}));
    assert_eq!(config["connections"]["local"]["requestTimeoutMs"], 300000);
    assert_eq!(config["connections"]["local"]["discoveryTimeoutMs"], 15000);
    let mut host = f::start(&directory, &config, "native-differential-host").await;
    let catalog = host::bounded(5000, host.client().list_tools(None)).await.unwrap();
    assert_eq!(catalog.tools.len(), 12);
    let mut names = catalog.tools.iter().map(|tool| tool.name.as_ref()).collect::<Vec<_>>();
    names.sort_unstable();
    let mut expected = ["health_check", "list_connections", "list_models", "get_model_capabilities",
        "generate_image", "edit_image", "get_job", "cancel_job", "list_outputs", "get_output_info",
        "generate_media", "get_media_output_info"];
    expected.sort_unstable();
    assert_eq!(names, expected);
    let editing = catalog.tools.iter().find(|tool| tool.name == "edit_image").unwrap();
    assert_eq!(editing.description.as_deref(), Some("Image-to-image, comic editing, in-image translation and authorized watermark removal. Provide reference_images and an explicit prompt; edit_region={x,y,width,height} selects a pixel rectangle on the first reference and requires native mask support. target_language requests in-image translation. upscale=true requests a larger native size through the same model, with exactly one reference and numeric size; no local resizing. Unsupported references/masks fail before submission; no paid fallback."));
    for (name, args) in [("health_check",json!({"probe":true})),("list_connections",json!({})),("list_models",json!({})),("get_model_capabilities",json!({"model":"image-model"}))] {
        let result = host.call(name,args).await; assert_ne!(result["isError"],true); structured(&result);
    }
    let generated = host.call("generate_image",json!({"prompt":"draw","request_id":"tools-generation","return_mode":"files_and_preview"})).await;
    assert_eq!(generated["isError"],false);
    assert!(generated["content"].as_array().unwrap().iter().any(|item|item["type"]=="image"));
    assert!(generated["structuredContent"].is_object());
    // JSON has no JavaScript symbol-key channel: the SDK result must be fully serializable.
    assert_eq!(serde_json::from_str::<Value>(&generated.to_string()).unwrap(),generated);
    let id = generated["structuredContent"]["outputs"][0]["output_id"].as_str().unwrap();
    let resource = host::bounded(5000,host.client().read_resource(ReadResourceRequestParams::new(format!("numera-image://outputs/{id}")))).await.unwrap();
    let resource = serde_json::to_value(resource).unwrap();
    assert_eq!(resource["contents"][0]["mimeType"],"image/png");
    assert_eq!(resource["contents"][0]["blob"],base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&bytes));
    let info = host.call("get_output_info",json!({"output_id":id,"preview":true})).await;
    assert!(info["content"].as_array().unwrap().iter().any(|item|item["type"]=="image"));
    let edited = host.call("edit_image",json!({"prompt":"edit","request_id":"tools-edit","reference_images":[{"type":"output_id","output_id":id}]})).await;
    assert_eq!(edited["isError"],false);
    assert_ne!(host.call("get_job",json!({"request_id":"tools-edit"})).await["isError"],true);
    assert_eq!(host.call("list_outputs",json!({"limit":1})).await["structuredContent"]["next_offset"],1);
    assert_eq!(host.call("cancel_job",json!({"request_id":"tools-edit"})).await["structuredContent"]["refund_verified"],false);
    assert_eq!(host.call("generate_image",json!({"prompt":""})).await["isError"],true);
    assert_eq!(host.call("get_output_info",json!({"output_id":"00000000-0000-4000-8000-000000000000"})).await["isError"],true);
    assert_eq!(posts.load(Ordering::SeqCst),2);
    host.close().await.unwrap(); wire.close().await;
}

pub async fn two_hosts() {
    let work = f::workspace(); let directory = f::directory(&work,"two-host-acceptance");
    let (original,png)=f::image("mcp-png");
    let bodies=Arc::new(Mutex::new(Vec::new())); let capture=bodies.clone();
    let wire=Provider::new(move |request,_| {let capture=capture.clone();let original=original.clone();async move {
        if request.method=="POST" {capture.lock().unwrap().push(request.json());Ok(Reply::json(json!({"data":[{"b64_json":original["base64"]}]})))}
        else {Ok(Reply::json(json!({"data":[{"id":"family/arbitrary"}]})))}
    }}).await;
    let config=f::config(&directory,json!({"fixture":{"adapter":"openai-images","baseUrl":format!("{}/v1",wire.origin),"auth":{"type":"none"},"defaultModel":"family/arbitrary"}}));
    let file=f::write_config(&directory,&config);
    let launch=|index| Launch {command:host::binary(),args:vec!["bridge".into()],cwd:f::root(),name:format!("host-{index}"),version:"1.0.0".into(),env:std::collections::HashMap::from([
        ("NUMERA_CONFIG".into(),file.to_str().unwrap().into()),("NUMERA_LOG_DIR".into(),directory.join("logs").to_str().unwrap().into())])};
    let mut a=Host::connect(&directory,launch(0),Era::Auto,false).await.unwrap();
    let mut b=Host::connect(&directory,launch(1),Era::Auto,false).await.unwrap();
    assert_eq!(a.launch.args,["bridge"]); assert_eq!(a.launch.name,"host-0");
    assert_eq!(b.launch.args,["bridge"]); assert_eq!(b.launch.name,"host-1");
    assert_eq!(host::bounded(5000,a.client().list_tools(None)).await.unwrap().tools.len(),12);
    let args=json!({"prompt":"تصویر بدون بازنویسی 123","request_id":"same-logical-id"});
    let (first,second)=tokio::join!(a.call_timeout("generate_image",args.clone(),10000),b.call_timeout("generate_image",args.clone(),10000));
    assert_eq!(bodies.lock().unwrap().len(),1);
    assert_eq!(bodies.lock().unwrap()[0]["model"],"family/arbitrary");
    assert_eq!(bodies.lock().unwrap()[0]["prompt"],args["prompt"]);
    assert_eq!(bodies.lock().unwrap()[0]["n"],1);
    let receipt=a.job("same-logical-id",false).await;
    assert_eq!(receipt["status"],"completed"); assert_eq!(receipt["outputs"][0]["width"],8); assert_eq!(receipt["outputs"][0]["height"],6); f::saved(&receipt,&png);
    let conflict=b.call("generate_image",json!({"prompt":"different","request_id":"same-logical-id"})).await;
    assert_eq!(conflict["isError"],true);assert!(conflict.to_string().contains("request_id_conflict"));
    assert_eq!(bodies.lock().unwrap().len(),1); assert!(first["isError"]!=true||second["isError"]!=true);
    let (a,b)=tokio::join!(a.close(),b.close()); a.unwrap();b.unwrap();wire.close().await;
}
