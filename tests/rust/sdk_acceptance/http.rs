use super::{fixtures as f,host::{self,Client,Era,Launch,Result,command,capture}};
use numera_gen_mcp::devtools::Owned;
use rmcp::{model::{ClientCapabilities,ClientConfig,Implementation,ProtocolVersion},service::serve_client_with_lifecycle_and_ct,transport::streamable_http_client::{StreamableHttpClientTransport,StreamableHttpClientTransportConfig}};
use serde_json::{Value,json};
use std::{sync::{Arc,Mutex},time::{Duration,Instant}};
use tokio::{io::AsyncReadExt,process::{ChildStderr,ChildStdout},task::JoinHandle,time::timeout};
use tokio_util::sync::CancellationToken;
const TOKEN:&str="synthetic-local-http-token-not-provider-key";

struct HttpChild{child:Option<Owned>,stderr:Arc<Mutex<Vec<u8>>>,diagnostics:Option<JoinHandle<Result<()>>>,stdout:Option<JoinHandle<Result<Vec<u8>>>>}
impl Drop for HttpChild{fn drop(&mut self){if let Some(mut child)=self.child.take(){if child.stop(Instant::now()+Duration::from_secs(2)).is_err(){eprintln!("ERROR: HTTP fixture child cleanup unverified");}}if let Some(task)=self.diagnostics.take(){task.abort();}if let Some(task)=self.stdout.take(){task.abort();}}}
async fn connect(url:&str)->Client{
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(6)).no_proxy().build().unwrap();
    let mut config=StreamableHttpClientTransportConfig::with_uri(url.to_owned());config.auth_header=Some(TOKEN.into());
    let transport=StreamableHttpClientTransport::with_client(client,config);
    let info=ClientConfig::new(ClientCapabilities::default(),Implementation::new("native-http-fixture","1")).with_protocol_version(ProtocolVersion::V_2025_11_25);
    host::bounded(8000,serve_client_with_lifecycle_and_ct(info,transport,Era::Legacy.lifecycle(),CancellationToken::new())).await.unwrap()
}
async fn call(client:&Client,name:&str,args:Value)->Value{
    let request=serde_json::from_value(json!({"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
    let handle=host::bounded(1000,client.send_request_with_option(request,rmcp::service::PeerRequestOptions::with_timeout(Duration::from_secs(5)))).await.unwrap();
    let result=host::bounded(6000,handle.await_response()).await.unwrap();let rmcp::model::ServerResult::CallToolResult(result)=result else{panic!("HTTP SDK tools/call returned unexpected result")};serde_json::to_value(result).unwrap()
}
pub async fn run(){
    let work=f::workspace();let directory=f::directory(&work,"native-http");let config=f::config(&directory,json!({"offline":{"adapter":"openai-images","baseUrl":"https://invalid.example/v1","auth":{"type":"none"},"defaultModel":"opaque/image","requestTimeoutMs":3000}}));let file=f::write_config(&directory,&config);
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let port=listener.local_addr().unwrap().port();drop(listener);
    let launch=Launch{command:host::binary(),args:vec!["http".into(),"--port".into(),port.to_string(),"--config".into(),file.to_str().unwrap().into()],cwd:directory.clone(),name:"native-http-fixture".into(),version:"1".into(),env:std::collections::HashMap::from([("NUMERA_HTTP_TOKEN".into(),TOKEN.into())])};
    let mut child=Owned::spawn(&mut command(&launch,&work.0)).unwrap();let pipes=child.pipes().unwrap();drop(pipes.stdin);
    let stderr=Arc::new(Mutex::new(Vec::new()));let diagnostics=tokio::spawn(capture(ChildStderr::from_std(pipes.stderr).unwrap(),stderr.clone()));let mut out=ChildStdout::from_std(pipes.stdout).unwrap();let stdout=tokio::spawn(async move{let mut bytes=Vec::new();(&mut out).take(1048577).read_to_end(&mut bytes).await.map_err(|_|"HTTP stdout read failed")?;if bytes.len()>1048576{return Err("HTTP stdout bound exceeded".into());}Ok(bytes)});
    let mut owner=HttpChild{child:Some(child),stderr,diagnostics:Some(diagnostics),stdout:Some(stdout)};
    host::bounded(8000,async{loop{if String::from_utf8_lossy(&owner.stderr.lock().unwrap()).contains("Local MCP HTTP:"){break;}assert!(!owner.child.as_mut().unwrap().exited().unwrap(),"HTTP startup exited before readiness");tokio::task::yield_now().await;}}).await;
    let url=format!("http://127.0.0.1:{port}/mcp");let mut clients=[connect(&url).await,connect(&url).await];
    for client in &clients{assert_eq!(host::bounded(5000,client.list_tools(None)).await.unwrap().tools.len(),12);let templates=host::bounded(5000,client.list_resource_templates(None)).await.unwrap();assert_eq!(templates.resource_templates.len(),2);assert_eq!(templates.resource_templates[0].uri_template,"numera-image://outputs/{id}");assert_eq!(templates.resource_templates[1].uri_template,"numera-gen://outputs/{id}");let health=call(client,"health_check",json!({"probe":false})).await;assert_eq!(health["structuredContent"]["generation_verified"],false);assert!(health["structuredContent"].get("connections").is_some());assert!(call(client,"list_models",json!({"refresh":false})).await["structuredContent"].is_object());assert_eq!(call(client,"list_outputs",json!({"limit":0})).await["isError"],true);}
    let web=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(3)).no_proxy().build().unwrap();
    for(auth,origin,status)in[(None,None,401),(Some("Bearer wrong"),None,401),(Some("Bearer synthetic-local-http-token-not-provider-key"),Some("https://evil.example"),403)]{let mut request=web.post(&url).header("Content-Type","application/json").body("{}");if let Some(auth)=auth{request=request.header("Authorization",auth);}if let Some(origin)=origin{request=request.header("Origin",origin);}let response=request.send().await.unwrap();assert_eq!(response.status().as_u16(),status);response.bytes().await.unwrap();}
    assert!(!std::fs::read_to_string(&file).unwrap().contains(TOKEN));assert!(!String::from_utf8_lossy(&owner.stderr.lock().unwrap()).contains(TOKEN));
    // rmcp close owns transport termination (including its session DELETE), then joins its worker.
    for client in &mut clients{host::bounded(5000,client.close()).await.unwrap();}drop(clients);
    #[cfg(unix)] { let pid=owner.child.as_ref().unwrap().id();assert_eq!(unsafe{libc::kill(pid as i32,libc::SIGTERM)},0);host::bounded(10000,async{while !owner.child.as_mut().unwrap().exited().unwrap(){tokio::time::sleep(Duration::from_millis(10)).await;}}).await; }
    #[cfg(windows)] { /* Windows has no POSIX SIGTERM; the anchored private Job is stopped below. */ }
    let pid=owner.child.as_ref().unwrap().id();
    let code=owner.child.as_mut().unwrap().stop(Instant::now()+Duration::from_secs(3)).unwrap();owner.child.take();
    #[cfg(unix)] {assert_eq!(unsafe{libc::kill(pid as i32,0)},-1,"HTTP child must be reaped");assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));}
    #[cfg(windows)] let _=pid;
    #[cfg(unix)] assert_eq!(code,0,"HTTP shutdown must be graceful on POSIX");
    #[cfg(windows)] let _=code;
    let stdout=timeout(Duration::from_secs(3),owner.stdout.take().unwrap()).await.unwrap().unwrap().unwrap();assert!(stdout.is_empty(),"HTTP mode must emit no stdout");
    timeout(Duration::from_secs(3),owner.diagnostics.take().unwrap()).await.unwrap().unwrap().unwrap();assert!(!String::from_utf8_lossy(&owner.stderr.lock().unwrap()).contains(TOKEN));
}
