mod common;
#[path = "common/config.rs"]
mod config_fixture;
#[path = "common/image.rs"]
mod image_fixture;
use common::Workspace;
use numera_image_gen_mcp::{files::{images::inspect,output::{self,Publication}},jobs::{Receipt,Store}};
use serde_json::json;

#[test]
fn filesystem_sql_gap_reconciles_original_bytes_uuid_and_refuses_a_changed_publication_target() {
    let workspace=Workspace::new();let raw=workspace.config("https://invalid.example/v1","openai-images");let image=inspect(image_fixture::original(),&raw,None).unwrap();let state=workspace.0.join("state");let mut store=Store::open(&state).unwrap();
    let (_,mut receipt)=store.prepare(Receipt::new(json!({"request_id":"gap","connection":"test","status":"prepared","outputs":[],"deviations":[],"generation_outcome":"not_submitted"})),"effective","account").unwrap();assert!(store.admit("gap","test",2,2).unwrap());assert!(store.claim("gap",Some(&receipt),true).unwrap());receipt=store.get("gap").unwrap();store.reserve("gap",1048576,2097152).unwrap();store.retain("gap",0,&image.bytes,Some(&receipt)).unwrap();
    let fault=rusqlite::Connection::open(state.join("receipts.sqlite")).unwrap();fault.execute_batch("CREATE TRIGGER reject_output BEFORE INSERT ON outputs BEGIN SELECT RAISE(FAIL,'Controlled output-index failure'); END;").unwrap();
    let publication=||Publication{id:"gap",index:0,subdir:Some("originals"),prefix:Some("verified"),deviations:&[],expected:Some(&receipt)};
    assert_eq!(output::save(&image,&raw,&mut store,publication()).unwrap_err().code,"storage_error");
    let retained=store.pending("gap").unwrap().remove(0);let target=retained.target.clone().unwrap();assert_eq!(std::fs::read(&target).unwrap(),image.bytes);assert_eq!(retained.bytes.as_ref().unwrap(),&image.bytes);
    fault.execute_batch("DROP TRIGGER reject_output").unwrap();
    std::fs::write(&target,b"changed fixture").unwrap();assert_eq!(output::save(&image,&raw,&mut store,publication()).unwrap_err().code,"output_file_error");assert!(store.list_outputs(0,25).unwrap().is_empty());
    std::fs::write(&target,&image.bytes).unwrap();let saved=output::save(&image,&raw,&mut store,publication()).unwrap();assert_eq!(saved["output_id"],retained.output_id);assert_eq!(saved["path"],target);assert_eq!(store.list_outputs(0,25).unwrap().len(),1);assert!(store.pending("gap").unwrap().is_empty());
    assert_eq!(output::save(&image,&raw,&mut store,publication()).unwrap(),saved);assert_eq!(store.list_outputs(0,25).unwrap().len(),1);
    store.commit("gap",0,&saved,&[],Some(&receipt)).unwrap();
    drop(fault);drop(store);let reopened=Store::open(&state).unwrap();assert_eq!(reopened.output(saved["output_id"].as_str().unwrap()).unwrap(),saved);
}
