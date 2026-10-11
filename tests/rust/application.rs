mod common;
#[path = "common/config.rs"]
mod config_fixture;
#[path = "common/image.rs"]
mod image_fixture;
#[path = "common/http.rs"]
mod http_fixture;
use common::Workspace;
use http_fixture::{Reply, Server};
use numera_image_gen_mcp::{config::{Config, ConfigReload}, logging::Logger, mcp::application::Application, runtime_paths::RuntimePaths, services::request};
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn public_tools_validate_arguments_store_outputs_and_reload_last_good_config() {
    let workspace = Workspace::new();
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, image_fixture::original());
    let server = Server::new(move |path| Reply {bytes: None,status:200,value: if path.contains("images/generations") {json!({"data":[{"b64_json":encoded}]})}else{json!({"data":[{"id":"opaque/image"},{"id":"another/image"}]})}}).await;
    let file = workspace.0.join("config.json");
    let raw=workspace.config(&format!("{}/v1",server.origin),"openai-images");
    std::fs::write(&file,raw.to_string()).unwrap();
    let config=Config::parse(&raw.to_string(),&file,&[],&HashMap::new()).unwrap();
    let args=vec!["--config".into(),file.to_string_lossy().into_owned()];
    let reload=ConfigReload::new(config.clone(),file.clone(),args,HashMap::new());
    let logger=Arc::new(Logger::new(&workspace.0.join("logs"),"ERROR"));
    let app=Application::new(config,Some(reload),logger.clone(),RuntimePaths::new(&workspace.0).unwrap(),HashMap::new(),CancellationToken::new()).unwrap();
    let signal=CancellationToken::new();
    let action=async {
        assert_eq!(app.call("health_check",json!({"probe":true}),&signal).await.unwrap()["generation_verified"],false);
        assert_eq!(app.call("list_connections",json!({}),&signal).await.unwrap()["connections"][0]["name"],"test");
        let listed=app.call("list_models",json!({"refresh":true,"limit":1}),&signal).await.unwrap();
        assert_eq!(listed["models"].as_array().unwrap().len(),1);
        assert_eq!(listed["next_offset"],1);
        let next=app.call("list_models",json!({"offset":1,"limit":1}),&signal).await.unwrap();
        assert_eq!(next["models"][0]["id"],"another/image");
        assert_eq!(app.call("get_model_capabilities",json!({"model":"opaque/image"}),&signal).await.unwrap()["account_availability"],"unknown");
        let generated=app.call("generate_image",json!({"request_id":"public-image","prompt":"fixture"}),&signal).await.unwrap();
        assert_eq!(generated["status"],"completed");
        let id=generated["outputs"][0]["output_id"].as_str().unwrap();
        assert_eq!(app.call("get_output_info",json!({"output_id":id}),&signal).await.unwrap()["verified"],true);
        assert_eq!(app.original(id).unwrap().bytes,image_fixture::original());
        assert_eq!(app.preview(id).unwrap()["mimeType"],"image/png");
        assert_eq!(app.call("list_outputs",json!({"limit":1}),&signal).await.unwrap()["outputs"].as_array().unwrap().len(),1);
        assert_eq!(app.call("get_job",json!({"request_id":"public-image","refresh":true}),&signal).await.unwrap()["outputs"],generated["outputs"]);
        assert_eq!(app.call("cancel_job",json!({"request_id":"public-image"}),&signal).await.unwrap()["upstream_requested"],false);
        assert_eq!(server.requests.lock().unwrap().iter().filter(|path| path.ends_with("images/generations")).count(), 1);
        for (name,args) in [("list_models",json!({"limit":0})),("list_outputs",json!({"offset":-1})),("health_check",json!({"probe":"yes"})),("get_model_capabilities",json!({"model":null})),("get_output_info",json!({"output_id":"not-uuid"})),("unknown",json!({}))] {
            assert_eq!(app.call(name,args,&signal).await.unwrap_err().code,"invalid_input");
        }
        let mut changed=raw.clone();changed["returnMode"]=json!("files_and_preview");
        std::fs::write(&file,changed.to_string()).unwrap();
        assert_eq!(app.return_mode().unwrap(),"files_and_preview");
        std::fs::write(&file,"not-json").unwrap();
        assert_eq!(app.return_mode().unwrap(),"files_and_preview");
    };
    let result=tokio::time::timeout(Duration::from_secs(8),action).await;
    app.shutdown();drop(app);drop(logger);server.close().await;result.unwrap();
}

#[test]
fn public_request_schema_rejects_invalid_inputs_without_coercion() {
    assert_eq!(request::parse(json!({"prompt":"café 日本語 🌍"})).unwrap()["count"],1);
    for value in [json!(null),json!({"prompt":""}),json!({"prompt":"ok","unexpected":true}),json!({"prompt":"ok","count":0}),json!({"prompt":"ok","count":1.5}),json!({"prompt":"ok","request_id":"../escape"}),json!({"prompt":"ok","model":""}),json!({"prompt":"ok","wait":"true"}),json!({"prompt":"ok","seed":1.5}),json!({"prompt":"ok","provider_options":[]}),json!({"prompt":"ok","reference_images":[{"type":"path","path":""}]}),json!({"prompt":"ok","edit_region":{"x":0,"y":0,"width":0,"height":1}}),json!({"prompt":"ok","target_language":" "}),json!({"prompt":"ok","return_mode":"url"}),json!({"prompt":"ok","output_format":"gif"})] {
        assert_eq!(request::parse(value).unwrap_err().code,"invalid_input");
    }
    for value in [json!({}),json!({"type":"unknown","path":"file"}),json!({"type":"output_id","output_id":"bad"}),json!({"type":"url","url":"bad"}),json!({"type":"path","path":"file","extra":true})] {
        assert_eq!(request::validate_source(&value).unwrap_err().code,"invalid_input");
    }
    for value in [json!({"type":"path","path":"fixture"}),json!({"type":"url","url":"https://fixture.example/image"}),json!({"type":"data_url","data_url":"data:image/png;base64,YQ=="}),json!({"type":"output_id","output_id":"123e4567-e89b-42d3-a456-426614174000"})] {
        request::validate_source(&value).unwrap();
    }
}
