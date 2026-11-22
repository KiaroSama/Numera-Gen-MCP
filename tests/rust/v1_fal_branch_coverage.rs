//! Deterministic branch coverage for `native/media/fal.rs` handles, status, cancel, and outputs.
use numera_gen_mcp::{
    compat::value::json,
    media::fal::{self, Cancel, Status},
};
use serde_json::{Map, Value as Sv, json as sj};

#[test]
fn v1_fal_handle_status_cancel_and_outputs_branch_coverage() {
    // Direct fal::handle valid and rejected tracking URL branches.
    let ok_h = fal::handle(
        "fal-ai/flux/dev",
        &sj!({
            "request_id": "req_1",
            "status_url": "https://queue.fal.run/fal-ai/flux/requests/req_1/status",
            "response_url": "/fal-ai/flux/requests/req_1",
            "cancel_url": "http://127.0.0.1:8080/fal-ai/flux/requests/req_1/cancel"
        }),
    )
    .unwrap();
    assert_eq!(ok_h.id, "req_1");
    let ok_h2 = fal::handle(
        "fal-ai/flux/dev",
        &sj!({
            "request_id": "req_2",
            "status_url": "http://localhost:9000/fal-ai/flux/requests/req_2/status",
            "response_url": "http://[::1]:9000/fal-ai/flux/requests/req_2",
            "cancel_url": null
        }),
    )
    .unwrap();
    assert_eq!(ok_h2.cancel_url, "/fal-ai/flux/requests/req_2/cancel");
    let huge = "a".repeat(4097);
    for bad_url in [
        sj!(123),
        sj!(huge.clone()),
        sj!("not a url"),
        sj!("https://queue.fal.run:8443/fal-ai/flux/requests/req_1/status"),
        sj!("https://evil.example/fal-ai/flux/requests/req_1/status"),
        sj!("https://user@queue.fal.run/fal-ai/flux/requests/req_1/status"),
        sj!("https://:pw@queue.fal.run/fal-ai/flux/requests/req_1/status"),
        sj!("https://queue.fal.run/fal-ai/flux/requests/other/status"),
        sj!("https://queue.fal.run/fal-ai/flux/requests/req_1/status?x=1"),
        sj!("https://queue.fal.run/fal-ai/flux/requests/req_1/status#f"),
    ] {
        assert!(
            fal::handle(
                "fal-ai/flux/dev",
                &sj!({"request_id": "req_1", "status_url": bad_url})
            )
            .is_err()
        );
    }
    for bad_id in ["", "bad/id", &"x".repeat(257)] {
        assert!(fal::handle("fal-ai/flux/dev", &sj!({"request_id": bad_id})).is_err());
    }

    // Direct fal::status and fal::cancel branches.
    assert_eq!(
        fal::status(&sj!({"request_id": "r1", "status": "IN_QUEUE"})).unwrap(),
        Status::InQueue
    );
    assert_eq!(
        fal::status(&sj!({"request_id": "r1", "status": "IN_PROGRESS", "error_type": null}))
            .unwrap(),
        Status::InProgress
    );
    assert_eq!(
        fal::status(&sj!({"request_id": "r1", "status": "COMPLETED", "error_type": "rate_limit"}))
            .unwrap(),
        Status::Completed {
            failed: true,
            error_type: Some("rate_limit".into())
        }
    );
    for bad in [
        sj!({"request_id": "r1", "status": "IN_QUEUE", "error": "boom"}),
        sj!({"request_id": "r1", "status": "IN_PROGRESS", "error_type": "x"}),
        sj!({"request_id": "r1", "status": "COMPLETED", "error_type": 42}),
        sj!({"request_id": "r1", "status": "COMPLETED", "error_type": "bad-char"}),
        sj!({"request_id": "r1", "status": "COMPLETED", "error_type": "a".repeat(129)}),
        sj!({"request_id": "r1", "status": "WEIRD"}),
    ] {
        assert!(fal::status(&bad).is_err());
    }
    assert_eq!(
        fal::cancel(&sj!({"status": "OK"}), 200).unwrap(),
        Cancel::Removed
    );
    assert_eq!(
        fal::cancel(&sj!({"status": "CANCELLATION_REQUESTED"}), 202).unwrap(),
        Cancel::Requested
    );
    assert_eq!(
        fal::cancel(&sj!({"status": "ALREADY_COMPLETED"}), 400).unwrap(),
        Cancel::AlreadyCompleted
    );
    assert_eq!(
        fal::cancel(&sj!({"status": "NOT_FOUND"}), 404).unwrap(),
        Cancel::NotFound
    );
    assert!(fal::cancel(&sj!({"status": "ERR"}), 500).is_err());

    // fal::outputs + collect branches (rodin textures, hunyuan3d model_urls/thumbnail, artifact caps).
    assert!(fal::outputs("fal-ai/flux/dev", &sj!(["not-object"])).is_err());
    let rodin_empty = fal::outputs(
        "fal-ai/hyper3d/rodin",
        &sj!({"model_mesh": {"url": "https://cdn.example/m.glb"}, "textures": []}),
    )
    .unwrap();
    assert_eq!(rodin_empty.len(), 1);
    let rodin_bad_url = fal::outputs(
        "fal-ai/hyper3d/rodin",
        &sj!({
            "model_mesh": {"url": "https://cdn.example/m.glb", "content_type": "x".repeat(257)},
            "textures": [{"url": "https://cdn.example/t\n.png"}, {"url": ""}]
        }),
    )
    .unwrap();
    assert!(rodin_bad_url[0].reported_mime.is_none());
    assert!(rodin_bad_url[1].error.is_some());
    assert!(rodin_bad_url[2].error.is_some());
    let too_many: Vec<Sv> = (0..65)
        .map(|i| sj!({"url": format!("https://cdn.example/{i}.png")}))
        .collect();
    assert!(
        fal::outputs(
            "fal-ai/hyper3d/rodin",
            &sj!({"model_mesh": {"url": "https://cdn.example/m.glb"}, "textures": too_many})
        )
        .is_err()
    );

    let hy = fal::outputs(
        "fal-ai/hunyuan3d-v3/text-to-3d",
        &sj!({
            "model_glb": {"url": "https://cdn.example/main.glb"},
            "model_urls": {
                "obj": {"url": "https://cdn.example/m.obj"},
                "fbx": null,
                "glb": {"url": "https://cdn.example/alt.glb"}
            },
            "thumbnail": {"url": "https://cdn.example/thumb.png"}
        }),
    )
    .unwrap();
    assert_eq!(hy.len(), 3);
    let hy_dup = fal::outputs(
        "fal-ai/hunyuan3d-v3/text-to-3d",
        &sj!({
            "model_glb": {"url": "https://cdn.example/same.glb"},
            "model_urls": {
                "glb": {"url": "https://cdn.example/same.glb"},
                "usdz": {"url": "https://cdn.example/m.usdz"}
            }
        }),
    )
    .unwrap();
    assert_eq!(hy_dup.len(), 1);
    let mut over16 = Map::new();
    for i in 0..17 {
        over16.insert(format!("k{i}"), sj!(null));
    }
    for bad_variants in [
        sj!("bad"),
        Sv::Object(over16),
        sj!({"stl": {"url": "https://cdn.example/a.stl"}}),
    ] {
        assert!(
            fal::outputs(
                "fal-ai/hunyuan3d-v3/text-to-3d",
                &sj!({"model_glb": {"url": "https://cdn.example/m.glb"}, "model_urls": bad_variants})
            )
            .is_err()
        );
    }

    // Gateway handle, status, cancel, and outputs branches.
    let gh = fal::gateway_handle(
        "fal-ai/flux/dev",
        &sj!({
            "id": "g1",
            "status_url": "/api/v1/fal/jobs/g1",
            "response_url": "https://gw.example/api/v1/fal/jobs/g1?result=true",
            "cancel_url": "http://gw.example/v1/fal/jobs/g1"
        }),
    )
    .unwrap();
    assert_eq!(gh.id, "g1");
    for bad_gw in [
        sj!(123),
        sj!(huge),
        sj!("not-a-url"),
        sj!("ftp://gw.example/v1/fal/jobs/g1"),
        sj!("https://queue.fal.run/v1/fal/jobs/g1"),
        sj!("https://u@gw.example/v1/fal/jobs/g1"),
        sj!("https://:p@gw.example/v1/fal/jobs/g1"),
        sj!("https://gw.example/v1/fal/jobs/other"),
        sj!("https://gw.example/v1/fal/jobs/g1?wrong=1"),
        sj!("https://gw.example/v1/fal/jobs/g1#frag"),
    ] {
        assert!(
            fal::gateway_handle("fal-ai/flux/dev", &sj!({"id": "g1", "status_url": bad_gw}))
                .is_err()
        );
    }
    assert_eq!(
        fal::gateway_status(&sj!({"id": "g1", "status": "SUBMITTED"})).unwrap(),
        Status::InQueue
    );
    assert_eq!(
        fal::gateway_status(&sj!({"id": "g1", "status": "IN_PROGRESS"})).unwrap(),
        Status::InProgress
    );
    assert_eq!(
        fal::gateway_status(&sj!({"id": "g1", "status": "FAILED", "error_type": "upstream"}))
            .unwrap(),
        Status::Completed {
            failed: true,
            error_type: Some("upstream".into())
        }
    );
    assert_eq!(
        fal::gateway_status(&sj!({"id": "g1", "status": "CANCELLED"})).unwrap(),
        Status::Completed {
            failed: true,
            error_type: Some("client_cancelled".into())
        }
    );
    for bad_gs in [
        sj!({"id": "g1", "status": "SUBMISSION_UNKNOWN"}),
        sj!({"id": "g1", "status": "IN_QUEUE", "error": "x"}),
        sj!({"id": "g1", "status": "COMPLETED", "error_type": "bad-type"}),
        sj!({"id": "g1", "status": "UNKNOWN_STATE"}),
    ] {
        assert!(fal::gateway_status(&bad_gs).is_err());
    }
    assert_eq!(
        fal::gateway_cancel(&sj!({"status": "CANCELLED"}), 200).unwrap(),
        Cancel::Removed
    );
    assert_eq!(
        fal::gateway_cancel(&sj!({"status": "OK"}), 200).unwrap(),
        Cancel::Removed
    );
    assert_eq!(
        fal::gateway_cancel(&sj!({"status": "CANCELLATION_REQUESTED"}), 202).unwrap(),
        Cancel::Requested
    );
    assert_eq!(
        fal::gateway_cancel(&sj!({"status": "ALREADY_COMPLETED"}), 400).unwrap(),
        Cancel::AlreadyCompleted
    );
    assert_eq!(
        fal::gateway_cancel(&sj!({"status": "NOT_FOUND"}), 404).unwrap(),
        Cancel::NotFound
    );
    assert_eq!(
        fal::gateway_cancel(&sj!({"error": "missing"}), 404).unwrap(),
        Cancel::NotFound
    );
    assert!(fal::gateway_cancel(&sj!({}), 404).is_err());
    assert!(fal::gateway_cancel(&sj!({"status": "ERR"}), 500).is_err());
    assert!(fal::gateway_outputs("fal-ai/flux/dev", &sj!("nope")).is_err());
    assert!(fal::gateway_outputs("fal-ai/flux/dev", &sj!({"output": "nope"})).is_err());
    let gw_out = fal::gateway_outputs(
        "fal-ai/flux/dev",
        &sj!({"output": {"images": [{"url": "https://cdn.example/1.png"}]}}),
    )
    .unwrap();
    assert_eq!(gw_out.len(), 1);
    let raw_dyn = json!({"outputField": "video", "outputArray": false, "outputKind": "video"});
    assert!(
        fal::gateway_handle_with_override(
            "fal-ai/custom-gw/v1",
            Some(&raw_dyn),
            &sj!({"id": "g2"})
        )
        .is_ok()
    );
    assert!(fal::contract_with_override("fal-ai/flux/dev", "text-to-image", None).is_ok());
    assert!(
        fal::prepare_with_override(
            "fal-ai/flux/dev",
            "text-to-image",
            None,
            Some("hi"),
            &[],
            &sj!({})
        )
        .is_ok()
    );
    assert!(
        fal::outputs_with_override(
            "fal-ai/flux/dev",
            None,
            &sj!({"images": [{"url": "https://cdn.example/a.png"}]})
        )
        .is_ok()
    );
    assert!(fal::handle_with_override("fal-ai/flux/dev", None, &sj!({"request_id": "r2"})).is_ok());
}
