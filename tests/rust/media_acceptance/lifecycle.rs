use super::{fixtures as f, host::{self, structured}, provider::{Gate, Provider, Reply}, support as s};
use serde_json::json;
use std::{collections::HashSet, sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}}};

pub async fn recovery_and_concurrency() {
    host::scenario(35000, async {
        let work = f::workspace(); let directory = f::directory(&work, "media-restart");
        let ready = Arc::new(AtomicBool::new(false)); let polls = Arc::new(AtomicUsize::new(0));
        let r = ready.clone(); let p = polls.clone();
        let (url, mime, bytes) = s::data("audio");
        let output = s::output(s::AUDIO, &url, mime);
        let wire = Provider::new(move |request, _| { let r=r.clone();let p=p.clone();let output=output.clone();async move {
            s::authenticated(&request);
            if request.method == "POST" { return Ok(Reply::json(s::handle(s::AUDIO,"resume"))); }
            if request.path.contains("/status") {p.fetch_add(1,Ordering::SeqCst);return Ok(Reply::status(json!({"request_id":"resume","status":if r.load(Ordering::SeqCst){"COMPLETED"}else{"IN_PROGRESS"}}),202));}
            Ok(Reply::json(output))
        }}).await;
        let config = s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        let mut first = s::start(&directory,&config,"media-before-restart").await;
        let mut request=s::request("restart",s::AUDIO,"text-to-audio",true,&[]);request["wait"]=json!(false);
        let pending=s::generate(&first,request.clone()).await;
        assert_eq!(pending["upstream_job"]["id"],"resume");
        assert!(["queued","running"].contains(&pending["status"].as_str().unwrap()));
        first.close().await.unwrap();
        ready.store(true,Ordering::SeqCst);
        let mut reopened=s::start(&directory,&config,"media-after-restart").await;
        let done=s::settle(&reopened,"restart").await;
        let id=s::verified(&reopened,&done,"audio",mime,&bytes).await;
        let replay=s::generate(&reopened,request.clone()).await;
        assert_eq!(replay["outputs"][0]["output_id"],id);
        assert_eq!(wire.posts(),1);
        let mut changed=request;changed["prompt"]=json!("different effective request");
        let conflict=reopened.call("generate_media",changed).await;
        assert_eq!(conflict["isError"],true);
        assert!(conflict.to_string().contains("request_id_conflict"));
        assert_eq!(wire.posts(),1);
        assert!(polls.load(Ordering::SeqCst)>0);
        reopened.close().await.unwrap();wire.close().await;
    }).await;
    host::scenario(35000, async {
        let work=f::workspace();let directory=f::directory(&work,"media-two-hosts");
        let (url,mime,bytes)=s::data("audio");let polls=Arc::new(AtomicUsize::new(0));
        let wire=s::normal_provider(s::AUDIO,s::output(s::AUDIO,&url,mime),polls).await;
        let config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        let mut instances=HashSet::new();let mut hosts=host::pair(&directory,&config,&mut instances).await;
        let request=s::request("concurrent",s::AUDIO,"text-to-audio",true,&[]);
        let (a,b)=tokio::join!(hosts[0].call_timeout("generate_media",request.clone(),10000),hosts[1].call_timeout("generate_media",request,10000));
        assert!(a["isError"]!=true||b["isError"]!=true,"one original owner must complete");
        let done=s::settle(&hosts[0],"concurrent").await;
        let id=s::verified(&hosts[0],&done,"audio",mime,&bytes).await;
        assert_eq!(hosts[1].job("concurrent",false).await["outputs"][0]["output_id"],id);
        assert_eq!(wire.posts(),1);
        host::close_pair(&mut hosts).await;wire.close().await;
    }).await;
}

pub async fn unknown() {
    host::scenario(25000, async {
        let work=f::workspace();let directory=f::directory(&work,"media-unknown-ack");
        let wire=Provider::new(|request,_|async move{s::authenticated(&request);assert_eq!(request.method,"POST");Ok(Reply::disconnect())}).await;
        let config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        let request=s::request("unknown",s::AUDIO,"text-to-audio",true,&[]);
        let mut host=s::start(&directory,&config,"media-unknown").await;
        let _=host.call("generate_media",request.clone()).await;
        let receipt=host.job("unknown",false).await;
        assert_eq!(receipt["status"],"outcome_unknown");
        assert_eq!(receipt["another_submission_may_charge"],true);
        let _=host.call("generate_media",request.clone()).await;
        let _=host.job("unknown",true).await;
        assert_eq!(wire.posts(),1);
        host.close().await.unwrap();
        let mut restarted=s::start(&directory,&config,"media-unknown-restart").await;
        let _=restarted.call("generate_media",request).await;
        assert_eq!(restarted.job("unknown",true).await["status"],"outcome_unknown");
        assert_eq!(wire.posts(),1);
        assert_eq!(wire.captured().len(),1,"unknown acknowledgement has no original handle to poll");
        restarted.close().await.unwrap();wire.close().await;
    }).await;
}

pub async fn cancellation() {
    for (status,body,outcome) in [(200,"OK","confirmed_queued_removal"),(202,"CANCELLATION_REQUESTED","requested"),(400,"ALREADY_COMPLETED","already_completed"),(404,"NOT_FOUND","unknown")] {
        host::scenario(30000, async {
            let work=f::workspace();let directory=f::directory(&work,&format!("media-cancel-{status}"));
            let completed=Arc::new(AtomicBool::new(false));let c=completed.clone();
            let (url,mime,_)=s::data("audio");let output=s::output(s::AUDIO,&url,mime);
            let wire=Provider::new(move|request,_|{let c=c.clone();let output=output.clone();async move{
                s::authenticated(&request);
                if request.method=="POST" {return Ok(Reply::json(s::handle(s::AUDIO,"cancel")));}
                if request.method=="PUT" {assert!(request.path.ends_with("/requests/cancel/cancel"));return Ok(Reply::status(json!({"status":body}),status));}
                if request.path.contains("/status") {return Ok(Reply::json(json!({"request_id":"cancel","status":if c.load(Ordering::SeqCst){"COMPLETED"}else{"IN_PROGRESS"}})));}
                Ok(Reply::json(output))
            }}).await;
            let config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
            let mut host=s::start(&directory,&config,"media-cancel").await;
            let mut request=s::request("cancel",s::AUDIO,"text-to-audio",true,&[]);request["wait"]=json!(false);
            let _=s::generate(&host,request).await;
            let cancelled=host.call("cancel_job",json!({"request_id":"cancel"})).await;
            assert_ne!(cancelled["isError"],true,"{cancelled}");
            let cancelled=structured(&cancelled);
            assert_eq!(cancelled["refund_verified"],false);
            assert_eq!(cancelled["upstream_cancellation"],outcome);
            assert_eq!(wire.posts(),1);
            assert_eq!(wire.captured().iter().filter(|r|r.method=="PUT").count(),1);
            if status==400||status==202 {
                completed.store(true,Ordering::SeqCst);
                let recovered=host.job("cancel",true).await;
                assert_ne!(recovered["status"],"outcome_unknown");
                assert_eq!(wire.posts(),1,"cancel/result race cannot submit again");
            }
            host.close().await.unwrap();wire.close().await;
        }).await;
    }
    held_cancel_race().await;
}

async fn held_cancel_race() {
    host::scenario(35000,async{
        let work=f::workspace();let directory=f::directory(&work,"media-held-cancel");
        let entered=Gate::new();let release=Gate::new();let arrived=entered.clone();let held=release.clone();
        let wire=Provider::new(move|request,stop|{let arrived=arrived.clone();let held=held.clone();async move{
            s::authenticated(&request);
            if request.method=="POST" {return Ok(Reply::json(s::handle(s::AUDIO,"held")));}
            if request.method=="PUT" {return Ok(Reply::status(json!({"status":"CANCELLATION_REQUESTED"}),202));}
            arrived.release();tokio::select!{_ = held.wait()=>{},_ = stop.cancelled()=>{}}
            Ok(Reply::json(json!({"request_id":"held","status":"IN_PROGRESS"})))
        }}).await;
        let config=s::config(&directory,&wire.origin,&[(s::AUDIO,"text-to-audio",&["audio"])]);
        let mut instances=HashSet::new();let mut hosts=host::pair(&directory,&config,&mut instances).await;
        {
            let arguments=s::request("held",s::AUDIO,"text-to-audio",true,&[]);
            let generation=hosts[0].call_timeout("generate_media",arguments,10000);
            tokio::pin!(generation);
            tokio::select!{_ = host::bounded(5000,entered.wait())=>{},_ = &mut generation=>panic!("media wait ended before the controlled status gate")}
            let cancelled=hosts[1].call("cancel_job",json!({"request_id":"held"})).await;
            assert_ne!(cancelled["isError"],true);assert_eq!(structured(&cancelled)["refund_verified"],false);
            assert_eq!(structured(&cancelled)["upstream_cancellation"],"requested");
            let _=host::bounded(2000,generation.as_mut()).await;
            release.release();
        }
        assert_eq!(wire.posts(),1);assert_eq!(wire.captured().iter().filter(|r|r.method=="PUT").count(),1);
        host::close_pair(&mut hosts).await;wire.close().await;
    }).await;
}
