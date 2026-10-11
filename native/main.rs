use numera_image_gen_mcp::{config::{self,ConfigReload},error::{NumeraError,Result},logging::Logger,runtime_paths::RuntimePaths,mcp::{application::Application,handler::Handler,transport::Stdio}};
use rmcp::ServiceExt;
use std::{collections::HashMap,path::{Path,PathBuf},sync::Arc,time::Duration};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn installation_root(executable:&Path)->Result<PathBuf> {
    let directory=executable.parent().ok_or_else(NumeraError::runtime)?;
    // Ready releases can live at the installation root or its explicit bin subdirectory.
    let root=if directory.file_name().is_some_and(|n|n=="bin"){directory.parent().ok_or_else(NumeraError::runtime)?}else{directory};
    std::fs::canonicalize(root).map_err(|_|NumeraError::new("invalid_configuration","Cannot resolve the installed application root."))
}

async fn bridge(args:Vec<String>,root:PathBuf,env:HashMap<String,String>,runtime:RuntimePaths)->Result<()> {
    let file=config::config_file(&args,&env,&root)?;
    let mut pinned=args;
    if let Some(file)=&file {pinned.extend(["--config".into(),file.to_string_lossy().into_owned()]);}
    let config=config::load_config(&pinned,&env,&root)?;
    let logger=Arc::new(Logger::with_retention(Path::new(config.value["logging"]["directory"].as_str().ok_or_else(NumeraError::runtime)?),config.value["logging"]["level"].as_str().unwrap_or("INFO"),config.value["logging"]["retentionDays"].as_u64().unwrap_or(14)));
    let reload=file.map(|file|ConfigReload::new(config.clone(),file,pinned,env.clone()));
    let stop=CancellationToken::new();
    let application=Arc::new(Application::new(config,reload,logger.clone(),runtime,env,stop.clone())?);
    let handler=Handler::new(application.clone())?;
    let transport=Stdio::new(128*1024*1024,stop.clone());
    let service=handler.serve_with_ct(transport,stop.clone()).await.map_err(|_|NumeraError::new("invalid_input","MCP initialization failed.").stage("protocol"))?;
    logger.log("INFO","MCP","Stdio server ready.",json!({"runtime":"Rust","version":env!("CARGO_PKG_VERSION")}));
    let cancellation=service.cancellation_token();
    let mut waiting=Box::pin(service.waiting());
    tokio::select! {
        result=&mut waiting=>{result.map_err(|_|NumeraError::runtime())?;},
        _=tokio::signal::ctrl_c()=>{
            application.shutdown();cancellation.cancel();
            tokio::time::timeout(Duration::from_secs(8),&mut waiting).await.map_err(|_|NumeraError::new("request_timeout","MCP shutdown deadline exceeded."))?.map_err(|_|NumeraError::runtime())?;
        }
    }
    application.shutdown();
    logger.log("INFO","MCP","Server stopped.",json!({}));Ok(())
}

fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.first().map(String::as_str),Some("--version"|"version")){println!("numera-image-gen-mcp {}",env!("CARGO_PKG_VERSION"));return;}
    if matches!(args.first().map(String::as_str),Some("--help"|"help")){println!("Numera Image-Gen MCP\nUsage: numera-image-gen-mcp bridge [--config ABSOLUTE_PATH]\nClaude launches bridge directly; no launcher or background service is required.\nConfiguration: adjacent .env/config.local.json, explicit --config or NUMERA_CONFIG.\nManaged temporary files and cache stay inside the installation root.");return;}
    let outcome=(|| {
        let executable=std::env::current_exe().map_err(|_|NumeraError::runtime())?;
        let root=installation_root(&executable)?;
        let runtime=RuntimePaths::new(&root)?;
        let mut env=std::env::vars().collect::<HashMap<_,_>>();
        for (key,value) in runtime.child_environment(){env.insert(key,value);}
        if args.first().is_some_and(|s|s=="register") {
            return numera_image_gen_mcp::cli::maintenance::register(&args[1..],&root,&executable);
        }
        // No target compiler/build/install or helper subprocess is required in production.
        let executor=tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|_|NumeraError::runtime())?;
        if args.first().is_some_and(|s|s=="readiness" || s=="diagnose") {
            let (report,ready)=executor.block_on(numera_image_gen_mcp::cli::maintenance::readiness(&args[1..],&root,&env,&runtime))?;
            println!("{report}");return if ready{Ok(())}else{Err(NumeraError::new("readiness_blocked","Read-only readiness has unresolved blockers."))};
        }
        let args=if args.first().is_some_and(|s|s=="bridge"){args[1..].to_vec()}else{args};
        executor.block_on(bridge(args,root,env,runtime))
    })();
    if let Err(error)=outcome {eprintln!("{}",json!({"level":"ERROR","component":"startup","message":"Server cannot start.","error":error}));std::process::exit(1);}
}
