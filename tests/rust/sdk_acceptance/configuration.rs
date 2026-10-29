use super::{fixtures as f, host::{self, Era, Host, Launch, structured}, provider::{Provider, Reply}};
use serde_json::{Value,json};
use std::{path::Path, sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}}};

fn installed(directory:&Path)->std::path::PathBuf {
    let install=directory.join("installation with spaces");std::fs::create_dir_all(install.join("bin")).unwrap();
    let target=install.join("bin").join(host::binary().file_name().unwrap());std::fs::copy(host::binary(),&target).unwrap();
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&target,std::fs::Permissions::from_mode(0o755)).unwrap(); }
    target
}
fn private_write(path:&Path,value:&Value){
    use std::io::Write;
    let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let mut file=options.open(path).unwrap();file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
}
fn models(result:&Value)->Value {json!(structured(result)["models"].as_array().unwrap().iter().map(|m|json!({"id":m["id"],"name":m["name"]})).collect::<Vec<_>>())}
fn launch(command:std::path::PathBuf,cwd:&Path,name:&str,args:Vec<String>)->Launch {
    Launch{command,args,cwd:cwd.to_path_buf(),name:name.into(),version:"1.0.0".into(),env:Default::default()}
}
pub async fn compact(){
    let work=f::workspace();let directory=f::directory(&work,"compact-acceptance");
    let (row,png)=f::image("png");assert_eq!(row["metadata"]["mime"],"image/png");assert_eq!(row["metadata"]["alpha"],true);
    let case=directory.join("adjacent-case");std::fs::create_dir(&case).unwrap();let binary=installed(&case);let install=binary.parent().unwrap().parent().unwrap().to_path_buf();
    let models_seen=Arc::new(Mutex::new(Vec::new()));let seen=models_seen.clone();let gets=Arc::new(AtomicUsize::new(0));let get_count=gets.clone();let key="synthetic-simple-private-key";
    let wire=Provider::new(move|request,_|{let seen=seen.clone();let get_count=get_count.clone();let row=row.clone();async move{
        if request.method=="GET"{get_count.fetch_add(1,Ordering::SeqCst);return Ok(Reply::json(json!({"data":[]})));}
        assert_eq!(request.header("authorization"),Some("Bearer synthetic-simple-private-key"));let body=request.json();seen.lock().unwrap().push(body["model"].clone());
        assert_eq!(request.path,"/custom/api/v1/images/generations");assert_eq!(body["prompt"],"image 日本語");
        assert_eq!(body["size"],if body["model"]=="custom/exact-one"{"1024x1024"}else{"1536x1024"});
        assert_eq!(body["quality"],if body["model"]=="custom/exact-one"{"medium"}else{"high"});assert_eq!(body["output_format"],"png");
        Ok(Reply::json(json!({"data":[{"b64_json":row["base64"]}]})))
    }}).await;
    private_write(&install.join("config.local.json"),&json!({"api_endpoint":format!("{}/custom/api/v1",wire.origin),"api_key":key,"models":[
        {"id":"custom/exact-one","name":"First friendly name","size":"1024x1024","quality":"medium","output_format":"png"},
        {"id":"custom/exact-two","name":"Second 日本語","size":"512x512","quality":"low","output_format":"jpeg"}]}));
    let expected=launch(binary,&case,"simple-config-fixture",vec!["bridge".into()]);let mut host=Host::connect(&case,expected.clone(),Era::Auto,false).await.unwrap();
    assert_eq!(host.launch.command,expected.command);assert_eq!(host.launch.cwd,case);assert_eq!(host.launch.args,["bridge"]);assert_eq!(host.launch.name,expected.name);assert_eq!(host.launch.version,expected.version);assert_eq!(host.launch.env,expected.env);
    let listed=host.call_timeout("list_models",json!({}),10000).await;assert_ne!(listed["isError"],true);
    assert_eq!(models(&listed),json!([{"id":"custom/exact-one","name":"First friendly name"},{"id":"custom/exact-two","name":"Second 日本語"}]));assert_eq!(gets.load(Ordering::SeqCst),0);
    let first=host.call_timeout("generate_image",json!({"prompt":"image 日本語","request_id":"simple-first"}),10000).await;
    let second=host.call_timeout("generate_image",json!({"prompt":"image 日本語","model":"custom/exact-two","request_id":"simple-second","size":"1536x1024","quality":"high","output_format":"png"}),10000).await;
    assert_eq!(first["structuredContent"]["status"],"partial");assert_eq!(first["structuredContent"]["requested_model"],"custom/exact-one");
    assert_eq!(second["structuredContent"]["status"],"partial");assert_eq!(second["structuredContent"]["requested_model"],"custom/exact-two");
    assert_eq!(*models_seen.lock().unwrap(),vec![json!("custom/exact-one"),json!("custom/exact-two")]);f::saved(&structured(&first),&png);
    assert!(!(json!([listed,first,second]).to_string()+&host.text()).contains(key));
    let duplicate=host.call_timeout("generate_image",json!({"prompt":"image 日本語","request_id":"simple-first"}),10000).await;
    assert_eq!(duplicate["structuredContent"]["status"],"partial");assert_eq!(models_seen.lock().unwrap().len(),2);
    let failed=host.call_timeout("generate_image",json!({"prompt":"private-error-prompt","request_id":"simple-first"}),10000).await;assert_eq!(failed["isError"],true);
    host.close().await.unwrap();assert!(!host.text().contains(key));assert_logs(&install,key);wire.close().await;
    compact_responses(&work,&directory,&png).await;
}
fn assert_logs(install:&Path,key:&str){
    let text=f::logs(&install.join("logs"));for private in[key,"private-error-prompt","image 日本語"]{assert!(!text.contains(private));}
    let events=text.lines().filter(|line|!line.trim().is_empty()).map(|line|serde_json::from_str::<Value>(line).expect("every actual compact log row must be JSON")).collect::<Vec<_>>();let tools=events.iter().filter(|e|e["component"]=="tool").collect::<Vec<_>>();
    assert!(tools.iter().any(|e|e["message"]=="Tool failed."&&e["code"]=="request_id_conflict"));
    for done in tools.iter().filter(|e|e["message"]!="Tool started."){assert!(done["duration_ms"].is_number());assert_eq!(done["run_id"].as_str().unwrap().len(),36);assert!(done["run_id"].as_str().unwrap().bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)||b==b'-'));
        assert!(tools.iter().any(|start|start["call_id"]==done["call_id"]&&start["run_id"]==done["run_id"]&&start["message"]=="Tool started."));}
    assert!(tools.iter().any(|e|e["tool"]=="list_models"&&e["message"]=="Tool completed."));
}
async fn compact_responses(work:&crate::common::Workspace,directory:&Path,png:&[u8]){
    let case=directory.join("responses-case");std::fs::create_dir(&case).unwrap();let posts=Arc::new(AtomicUsize::new(0));let count=posts.clone();let bytes=png.to_vec();
    let wire=Provider::new(move|request,_|{let count=count.clone();let bytes=bytes.clone();async move{
        let post=count.fetch_add(1,Ordering::SeqCst)+1;assert_eq!(request.path,"/deployment/v1/responses");let body=request.json();assert_eq!(body["model"],"opaque/orchestrator");
        assert_eq!(body["input"][0]["content"],json!([{"type":"input_text","text":"Responses 日本語"}]));
        assert_eq!(body["tools"],json!([{"type":"image_generation","action":"generate","model":"opaque/image","output_format":"png","size":if post==1{"1024x1024"}else{"1536x1024"},"quality":if post==1{"medium"}else{"high"}}]));
        assert_eq!(body["tool_choice"],json!({"type":"image_generation"}));Ok(Reply::json(json!({"output":[{"type":"image_generation_call","status":"completed","result":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&bytes)}]})))
    }}).await;
    let config=json!({"api_endpoint":format!("{}/deployment/v1",wire.origin),"api_key":"synthetic-responses-key","profile":"openai-responses","orchestration_model":"opaque/orchestrator","models":[{"id":"opaque/image","name":"Friendly","output_format":"png","size":"1024x1024","quality":"medium"}]});
    let file=f::write_config(&case,&config);let mut expected=launch(host::binary(),&f::root(),"responses-config-fixture",vec!["bridge".into(),"--config".into(),file.to_str().unwrap().into()]);expected.version="1".into();let mut host=Host::connect(&work.0,expected.clone(),Era::Auto,false).await.unwrap();assert_eq!(host.launch.command,expected.command);assert_eq!(host.launch.args,expected.args);assert_eq!(host.launch.cwd,expected.cwd);assert_eq!(host.launch.name,expected.name);
    for(id,extra)in[("defaults",json!({})),("override",json!({"size":"1536x1024","quality":"high"}))]{let mut args=json!({"prompt":"Responses 日本語","request_id":id});args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());let result=host.call_timeout("generate_image",args,10000).await;
        assert_ne!(result["isError"],true);assert_eq!(result["structuredContent"]["requested_model"],"opaque/image");assert_eq!(result["structuredContent"]["status"],"partial");f::saved(&structured(&result),png);}
    assert_eq!(posts.load(Ordering::SeqCst),2);host.close().await.unwrap();wire.close().await;
}
pub async fn dotenv(){
    let work=f::workspace();let directory=f::directory(&work,"env-acceptance");let command=installed(&directory);let install=command.parent().unwrap().parent().unwrap().to_path_buf();let file=install.join(".env");let key="synthetic-env-private-key";
    let text=|id:&str,name:&str|format!("API_ENDPOINT=https://api.example/v1\nAPI_KEY={key}\nMODEL_1_ID={id}\nMODEL_1_NAME=\"{name}\"\n");
    std::fs::write(&file,text("first/model","First 日本語")).unwrap();let mut expected=launch(command,&directory,"env-stdio-fixture",vec!["bridge".into()]);expected.version="1".into();let mut host=Host::connect(&work.0,expected,Era::Auto,false).await.unwrap();
    assert_eq!(host.launch.cwd,directory);assert_eq!(host.launch.args,["bridge"]);assert_eq!(host.launch.name,"env-stdio-fixture");assert!(host.launch.env.is_empty());
    let first=host.call_timeout("list_models",json!({"refresh":false}),10000).await;assert_ne!(first["isError"],true);assert_eq!(models(&first),json!([{"id":"first/model","name":"First 日本語"}]));
    std::fs::write(&file,text("second/model","Second فارسی")).unwrap();let second=host.call_timeout("list_models",json!({"refresh":false}),10000).await;assert_ne!(second["isError"],true);assert_eq!(models(&second),json!([{"id":"second/model","name":"Second فارسی"}]));
    std::fs::write(&file,"API_KEY=\n").unwrap();let invalid=host.call_timeout("list_models",json!({"refresh":false}),10000).await;assert_ne!(invalid["isError"],true);assert_eq!(structured(&invalid)["models"].as_array().unwrap().iter().map(|m|m["id"].clone()).collect::<Vec<_>>(),vec![json!("second/model")]);
    assert!(!(json!([first,second]).to_string()+&host.text()).contains(key));host.close().await.unwrap();assert!(!host.text().contains(key));assert!(!f::logs(&install.join("logs")).contains(key));
}
pub async fn negotiation(){
    let work=f::workspace();let directory=f::directory(&work,"negotiation-acceptance");
    for(name,era)in[("legacy",Era::Legacy),("auto",Era::Auto),("pin",Era::Pin)]{let case=directory.join(name);std::fs::create_dir(&case).unwrap();let mut config=f::config(&case,json!({}));config["logging"]["level"]=json!("ERROR");let file=f::write_config(&case,&config);let result=Host::connect(&case,Launch::bridge(&case,&file,"era-fixture"),era,false).await;
        if name=="pin"{assert_eq!(result.err().unwrap(),"ERA_NEGOTIATION_FAILED");}else{let mut host=result.unwrap();assert_eq!(host::bounded(5000,host.client().list_tools(None)).await.unwrap().tools.len(),12);assert_eq!(serde_json::to_value(host.client().peer_info().unwrap().as_ref()).unwrap()["protocolVersion"],"2025-11-25");host.close().await.unwrap();}}
    let file=directory.join("auto/config.json");let result=Host::connect(&directory,Launch::bridge(&directory,&file,"cancelled-handshake"),Era::Legacy,true).await;assert_eq!(result.err().unwrap(),"HANDSHAKE_CANCELLED_AND_CHILD_REAPED","pre-aborted handshake must still spawn and reap its actual child");
}
