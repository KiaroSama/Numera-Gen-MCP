use super::{fixtures as f,host::{self,structured},provider::{Gate,Provider,Reply},support as s};
use serde_json::json;
use std::{collections::HashSet,sync::{Arc,atomic::{AtomicUsize,Ordering}},time::{Duration,Instant}};

pub async fn run() {
    defaults().await;
    no_reference_io_before_contract_validation().await;
    absolute_poll_deadline().await;
    cancelled_completed_result_is_recoverable().await;
    changed_account_still_cancels_the_local_waiter().await;
}

async fn changed_account_still_cancels_the_local_waiter() {
    host::scenario(35000, async {
        let work = f::workspace();
        let directory = f::directory(&work, "media-local-cancel-without-account");
        let entered = Gate::new(); let release = Gate::new();
        let arrived = entered.clone(); let held = release.clone();
        let wire = Provider::new(move |request, stop| {
            let arrived = arrived.clone(); let held = held.clone();
            async move {
                s::authenticated(&request);
                if request.method == "POST" { return Ok(Reply::json(s::handle(s::AUDIO, "held-account"))); }
                assert_eq!(request.method, "GET", "changed account must never receive a cancellation PUT");
                arrived.release();
                tokio::select! { _ = held.wait() => {}, _ = stop.cancelled() => {} }
                Ok(Reply::json(json!({"request_id":"held-account","status":"IN_PROGRESS"})))
            }
        }).await;
        let config = s::config(&directory, &wire.origin, &[(s::AUDIO, "text-to-audio", &["audio"])]);
        let mut first = s::start(&directory, &config, "media-original-account").await;
        {
            let waiting = first.call_timeout("generate_media", s::request("local-stop", s::AUDIO, "text-to-audio", true, &[]), 10000);
            tokio::pin!(waiting);
            tokio::select! { _ = host::bounded(5000, entered.wait()) => {}, _ = &mut waiting => panic!("expected a held original-account waiter") }
            let mut changed_config = config.clone();
            changed_config["connections"]["local"]["auth"]["apiKey"] = json!("synthetic-other-account-key");
            let mut changed = s::start(&directory, &changed_config, "media-changed-account").await;
            let result = changed.call("cancel_job", json!({"request_id":"local-stop"})).await;
            assert_eq!(result["isError"], true, "targeted provider cancellation requires the original account");
            assert!(result.to_string().contains("permission_denied"));
            let _ = host::bounded(2000, waiting.as_mut()).await;
            assert_eq!(wire.posts(), 1);
            assert!(!wire.captured().iter().any(|request| request.method == "PUT"));
            assert_ne!(changed.job("local-stop", false).await["status"], "completed");
            release.release();
            changed.close().await.unwrap();
        }
        first.close().await.unwrap(); wire.close().await;
    }).await;
}

async fn defaults() {
    host::scenario(30000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-effective-defaults");
        let (url,mime,bytes)=s::data("audio");
        let wire=s::normal_provider(s::AUDIO,s::output(s::AUDIO,&url,mime),Arc::new(AtomicUsize::new(0))).await;
        let mut config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        config["connections"]["local"]["defaults"]=json!({"seconds_total":3,"steps":20});
        config["connections"]["local"]["modelOverrides"][s::AUDIO]["defaults"]=json!({"seconds_total":2,"steps":30});
        let mut first=s::start(&directory,&config,"media-defaults-first").await;
        let mut request=s::request("effective-defaults",s::AUDIO,"text-to-audio",true,&[]);
        request["parameters"]=json!({"steps":40});
        let done=s::generate(&first,request.clone()).await;
        s::verified(&first,&done,"audio",mime,&bytes).await;
        let calls=wire.captured();let body=calls.iter().find(|r|r.method=="POST").unwrap().json();
        assert_eq!(body["seconds_total"],2);assert_eq!(body["steps"],40);
        first.close().await.unwrap();
        config["connections"]["local"]["modelOverrides"][s::AUDIO]["defaults"]["seconds_total"]=json!(4);
        let mut changed=s::start(&directory,&config,"media-defaults-changed").await;
        let conflict=changed.call("generate_media",request).await;
        assert_eq!(conflict["isError"],true);assert!(conflict.to_string().contains("request_id_conflict"));
        assert_eq!(wire.posts(),1,"changed effective defaults may not reuse or replay a charged request");
        changed.close().await.unwrap();wire.close().await;
    }).await;
}

async fn no_reference_io_before_contract_validation() {
    host::scenario(25000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-contract-before-assets");
        let (url,mime,_)=s::data("audio");let output=s::output(s::AUDIO_AUDIO,&url,mime);
        let asset=Provider::new(|_,_|async{Err("Unsupported roles/parameters must reject before reference GET".into())}).await;
        let wire=s::normal_provider(s::AUDIO_AUDIO,output,Arc::new(AtomicUsize::new(0))).await;
        let mut config=s::config(&directory,&wire.origin,&[(s::AUDIO_AUDIO,"audio-to-audio",&["audio"])]);
        config["files"]["assetAllowances"]=json!([{"origin":asset.origin,"pathPrefix":"/approved.wav"}]);
        let mut host=s::start(&directory,&config,"media-preflight-before-get").await;
        for (index,role,parameters) in [(0,"mask",json!({})),(1,"audio",json!({"audio_url":"https://example.invalid/override"})),(2,"audio",json!({"total_seconds":0}))] {
            let request=json!({"request_id":format!("preflight-{index}"),"model":s::AUDIO_AUDIO,"mode":"audio-to-audio","prompt":"x","parameters":parameters,
                "references":[{"kind":"audio","role":role,"url":format!("{}/approved.wav",asset.origin)}]});
            let result=host.call("generate_media",request).await;assert_eq!(result["isError"],true);
        }
        assert!(asset.captured().is_empty());assert!(wire.captured().is_empty());
        host.close().await.unwrap();wire.close().await;asset.close().await;
    }).await;
}

async fn absolute_poll_deadline() {
    host::scenario(25000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-poll-absolute-deadline");
        let entered=Gate::new();let release=Gate::new();let arrived=entered.clone();let held=release.clone();
        let wire=Provider::new(move|request,stop|{let arrived=arrived.clone();let held=held.clone();async move{
            s::authenticated(&request);
            if request.method=="POST" {return Ok(Reply::json(s::handle(s::AUDIO,"deadline")));}
            arrived.release();tokio::select!{_=held.wait()=>{},_=stop.cancelled()=>{}}
            Ok(Reply::json(json!({"request_id":"deadline","status":"IN_PROGRESS"})))
        }}).await;
        let mut config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        config["connections"]["local"]["requestTimeoutMs"]=json!(1200);
        config["connections"]["local"]["discoveryTimeoutMs"]=json!(10000);
        let mut host=s::start(&directory,&config,"media-original-deadline").await;
        let started=Instant::now();
        let result=host.call_timeout("generate_media",s::request("deadline",s::AUDIO,"text-to-audio",true,&[]),5000).await;
        assert!(started.elapsed()<Duration::from_secs(4),"status I/O must not receive a fresh ten-second budget");
        host::bounded(1000,entered.wait()).await;
        let receipt=if result["isError"]==true {host.job("deadline",false).await}else{structured(&result)};
        assert_eq!(receipt["status"],"running");assert_eq!(receipt["upstream_job"]["id"],"deadline");
        assert_eq!(wire.posts(),1);release.release();host.close().await.unwrap();wire.close().await;
    }).await;
}

async fn cancelled_completed_result_is_recoverable() {
    host::scenario(35000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-completed-after-cancel");
        let entered=Gate::new();let release=Gate::new();let arrived=entered.clone();let held=release.clone();
        let result_calls=Arc::new(AtomicUsize::new(0));let calls=result_calls.clone();
        let (url,mime,bytes)=s::data("audio");let output=s::output(s::AUDIO,&url,mime);
        let wire=Provider::new(move|request,stop|{let arrived=arrived.clone();let held=held.clone();let calls=calls.clone();let output=output.clone();async move{
            s::authenticated(&request);
            if request.method=="POST" {return Ok(Reply::json(s::handle(s::AUDIO,"complete-race")));}
            if request.method=="PUT" {return Ok(Reply::status(json!({"status":"ALREADY_COMPLETED"}),400));}
            if request.path.contains("/status") {return Ok(Reply::json(json!({"request_id":"complete-race","status":"COMPLETED"})));}
            calls.fetch_add(1,Ordering::SeqCst);arrived.release();tokio::select!{_=held.wait()=>{},_=stop.cancelled()=>{}}
            Ok(Reply::json(output))
        }}).await;
        let config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        let mut instances=HashSet::new();let mut hosts=host::pair(&directory,&config,&mut instances).await;
        {
            let arguments=s::request("complete-race",s::AUDIO,"text-to-audio",true,&[]);
            let generation=hosts[0].call_timeout("generate_media",arguments,10000);
            tokio::pin!(generation);
            tokio::select!{_=host::bounded(5000,entered.wait())=>{},_=&mut generation=>panic!("expected a held completed-result request")}
            let cancelled=hosts[1].call("cancel_job",json!({"request_id":"complete-race"})).await;
            assert_eq!(structured(&cancelled)["upstream_cancellation"],"already_completed");
            let stopped=host::bounded(2000,generation.as_mut()).await;
            let stopped=if stopped["isError"]==true{hosts[1].job("complete-race",false).await}else{structured(&stopped)};
            assert_ne!(stopped["status"],"completed","held result was not released before targeted local cancellation");
            release.release();
        }
        let completed=s::settle(&hosts[1],"complete-race").await;
        let output_id=s::verified(&hosts[1],&completed,"audio",mime,&bytes).await;
        let again=hosts[0].job("complete-race",true).await;
        assert_eq!(again["outputs"][0]["output_id"],output_id);
        assert_eq!(wire.posts(),1);assert!(result_calls.load(Ordering::SeqCst)<=2);
        host::close_pair(&mut hosts).await;wire.close().await;
    }).await;
}
