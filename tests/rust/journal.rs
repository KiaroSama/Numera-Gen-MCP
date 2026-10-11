mod common;
#[path = "common/image.rs"]
mod image_fixture;
use common::Workspace;
use numera_image_gen_mcp::jobs::{Receipt, Store};
use serde_json::json;

fn receipt(id: &str) -> Receipt {
    Receipt::new(json!({"request_id":id,"connection":"test","status":"prepared","generation_outcome":"not_submitted","outputs":[],"errors":[],"warnings":[],"deviations":[]}))
}

#[test]
fn journal_reservations_urls_and_stable_publication_fail_closed_under_writer_fences() {
    let workspace=Workspace::new();
    let mut store=Store::open(&workspace.0.join("state")).unwrap();
    let (_,mut original)=store.prepare(receipt("first"),"effective","account").unwrap();
    assert!(store.admit("first","test",4,4).unwrap());
    original=store.get("first").unwrap();
    for (amount,budget) in [(0,100),(-1,100),(10,0),(9007199254740992,9007199254740992)] {
        assert_eq!(store.reserve("first",amount,budget).unwrap_err().code,"invalid_configuration");
    }
    store.reserve("first",1048576,2097152).unwrap();
    store.retain_url("first",0,"https://fixture.example/original",Some(&original)).unwrap();
    assert_eq!(store.pending("first").unwrap()[0].phase,"waiting");
    assert_eq!(store.publication("first",0,|_|Ok("unused".into()),Some(&original)).unwrap_err().code,"output_file_error");
    let bytes=image_fixture::original();
    store.retain("first",0,&bytes,Some(&original)).unwrap();
    let stable=store.pending("first").unwrap()[0].output_id.clone();
    store.retain_url("first",0,"https://fixture.example/other",Some(&original)).unwrap();
    assert_eq!(store.pending("first").unwrap()[0].output_id,stable);
    assert!(store.pending("first").unwrap()[0].url.is_none());
    assert_eq!(store.retain("first",0,b"different",Some(&original)).unwrap_err().code,"invalid_response");
    assert_eq!(store.retain("first",10,&bytes,Some(&original)).unwrap_err().code,"invalid_response");
    let item=store.publication("first",0,|_|Ok(workspace.0.join("original.png").to_string_lossy().into_owned()),Some(&original)).unwrap();
    let same=store.publication("first",0,|_|panic!("target cannot change"),Some(&original)).unwrap();
    assert_eq!(item.target,same.target);
    assert_eq!(store.commit("first",0,&json!({"output_id":"wrong","path":"wrong","sha256":"wrong"}),&[],Some(&original)).unwrap_err().code,"output_file_error");
    store.save_continuation("first","scope",&json!({"signature":"synthetic"})).unwrap();
    assert_eq!(store.continuation("first","scope").unwrap(),json!({"signature":"synthetic"}));
    assert_eq!(store.continuation("first","another").unwrap_err().code,"permission_denied");
    assert!(store.continuation("absent","scope").unwrap().is_null());
    let (_,second)=store.prepare(receipt("second"),"second","account").unwrap();
    assert!(store.admit("second","test",4,4).unwrap());
    assert_eq!(store.reserve("second",1048576,1048576).unwrap_err().code,"rate_limited");
    store.release("second",Some(&second)).unwrap();
    store.release("first",Some(&original)).unwrap();
    assert_eq!(store.retain("first",1,&bytes,Some(&original)).unwrap_err().code,"outcome_unknown");
}
