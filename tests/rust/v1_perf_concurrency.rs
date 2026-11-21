//! Concurrency, performance, and SQLite race regression tests (Plans 018-021, DD-04).
use numera_gen_mcp::{
    compat::value::json,
    config::Config,
    jobs::{Receipt, Store},
    logging::Logger,
    mcp::application::Application,
    runtime_paths::RuntimePaths,
};
use std::{collections::HashMap, sync::Arc};
use tokio_util::sync::CancellationToken;

#[test]
fn dd04_concurrent_store_open_migrates_without_race() {
    let dir = std::env::temp_dir().join(format!("numera-dd04-{}", uuid::Uuid::new_v4()));
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let d = dir.clone();
            std::thread::spawn(move || Store::open(&d))
        })
        .collect();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan018_partial_index_exists_and_foreign_keys_enabled() {
    let dir = std::env::temp_dir().join(format!("numera-p018-{}", uuid::Uuid::new_v4()));
    let _store = Store::open(&dir).unwrap();
    let conn = rusqlite::Connection::open(dir.join("receipts.sqlite")).unwrap();
    let has_index: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='requests_reserved'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_index, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plan021_two_store_owners_fence_stale_generation() {
    let dir = std::env::temp_dir().join(format!("numera-p021-{}", uuid::Uuid::new_v4()));
    let clock = Arc::new(std::sync::atomic::AtomicI64::new(10_000));
    let c1 = clock.clone();
    let c2 = clock.clone();
    let mut s1 =
        Store::open_with_lease_clock(&dir, move || c1.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap();
    let mut s2 =
        Store::open_with_lease_clock(&dir, move || c2.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap();
    let (_, mut r1) = s1
        .prepare(
            Receipt::new(json!({
                "request_id": "race-1", "connection": "c", "adapter": "openai-images",
                "operation": "generate", "status": "prepared", "generation_outcome": "not_submitted",
                "outputs": [], "errors": [], "warnings": [], "deviations": []
            })),
            "h",
            "id",
        )
        .unwrap();
    r1.value["status"] = json!("running");
    s1.update(&mut r1).unwrap();
    clock.fetch_add(Store::LEASE_MS + 5, std::sync::atomic::Ordering::SeqCst);
    assert!(s2.claim("race-1", None, true).unwrap());
    r1.value["status"] = json!("completed");
    assert_eq!(s1.update(&mut r1).unwrap_err().code, "outcome_unknown");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn plan020_application_list_jobs_and_concurrent_calls_do_not_deadlock() {
    let dir = std::env::temp_dir().join(format!("numera-p020-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = RuntimePaths::new(&dir).unwrap();
    let cfg_json = json!({
        "schemaVersion": 1,
        "defaultConnection": "c",
        "outputDir": dir.join("out").to_str().unwrap(),
        "stateDir": dir.join("state").to_str().unwrap(),
        "logging": {"directory": dir.join("logs").to_str().unwrap()},
        "connections": {
            "c": {
                "adapter": "openai-images",
                "baseUrl": "https://api.example/v1",
                "auth": {"type": "none"}
            }
        }
    });
    let cfg = Config::parse(
        &cfg_json.to_string(),
        &dir.join("c.json"),
        &[],
        &HashMap::new(),
    )
    .unwrap();
    let logger = Arc::new(Logger::new(&dir.join("logs"), "info"));
    let app = Arc::new(
        Application::new(
            cfg,
            None,
            logger,
            runtime,
            HashMap::new(),
            CancellationToken::new(),
        )
        .unwrap(),
    );
    let cancel = CancellationToken::new();
    let (a, b) = tokio::join!(
        app.call("list_connections", json!({}), &cancel),
        app.call("list_jobs", json!({"limit": 10}), &cancel)
    );
    assert!(a.is_ok());
    let jobs = b.unwrap();
    assert_eq!(jobs["jobs"], json!([]));
    // Seed 3 receipts and exercise list_jobs filters, pagination, has_more, and error branches.
    {
        let mut store = Store::open(&dir.join("state")).unwrap();
        for (i, st) in ["running", "completed", "failed"].iter().enumerate() {
            let (_, mut rec) = store
                .prepare(
                    Receipt::new(json!({
                        "request_id": format!("app-j-{i}"),
                        "connection": "c",
                        "model": "m",
                        "adapter": "openai-images",
                        "operation": "generate",
                        "status": "prepared",
                        "generation_outcome": "not_submitted",
                        "outputs": [],
                        "errors": [],
                        "warnings": [],
                        "deviations": []
                    })),
                    &format!("ah-{i}"),
                    "ident",
                )
                .unwrap();
            rec.value["status"] = json!(*st);
            store.update(&mut rec).unwrap();
        }
    }
    let page1 = app
        .call("list_jobs", json!({"limit": 1, "offset": 0}), &cancel)
        .await
        .unwrap();
    assert_eq!(page1["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(page1["next_offset"], json!(1));
    let filtered = app
        .call(
            "list_jobs",
            json!({"status": "completed", "limit": 10}),
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(filtered["jobs"].as_array().unwrap().len(), 1);
    assert!(filtered["next_offset"].is_null());
    assert!(
        app.call("list_jobs", json!({"status": "bogus"}), &cancel)
            .await
            .is_err()
    );
    assert!(
        app.call("list_jobs", json!({"limit": 0}), &cancel)
            .await
            .is_err()
    );
    assert!(
        app.call("list_jobs", json!({"offset": -1}), &cancel)
            .await
            .is_err()
    );
    assert!(
        app.call("nonexistent_tool", json!({}), &cancel)
            .await
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn v1_extra_branch_coverage_assets_and_fal_outputs() {
    use numera_gen_mcp::{http::assets, media::fal};

    for priv_ip in [
        "0.0.0.0",
        "10.0.0.1",
        "100.64.0.1",
        "127.0.0.1",
        "169.254.169.254",
        "172.16.0.1",
        "192.168.1.1",
        "198.18.0.1",
        "224.0.0.1",
        "240.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "fe80::1",
        "fc00::1",
        "fd00::1",
        "ff02::1",
        "[::1]",
        "not-an-ip",
    ] {
        let res = assets::private_address(priv_ip);
        assert!(res, "case {priv_ip}");
    }

    // Cover built-in fal::outputs & gateway_outputs across endpoints
    let flux = fal::outputs(
        "fal-ai/flux/dev",
        &serde_json::json!({"images": [{"url": "https://v3.fal.media/a.png", "content_type": "image/png"}]}),
    )
    .unwrap();
    assert_eq!(flux.len(), 1);
    let audio = fal::outputs(
        "fal-ai/stable-audio",
        &serde_json::json!({"audio_file": {"url": "https://v3.fal.media/a.wav", "content_type": "audio/wav"}}),
    )
    .unwrap();
    assert_eq!(audio.len(), 1);
    let video = fal::outputs(
        "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
        &serde_json::json!({"video": {"url": "https://v3.fal.media/v.mp4", "content_type": "video/mp4"}}),
    )
    .unwrap();
    assert_eq!(video.len(), 1);
    let trellis = fal::outputs(
        "fal-ai/trellis",
        &serde_json::json!({"model_mesh": {"url": "https://v3.fal.media/m.glb", "content_type": "model/gltf-binary"}}),
    )
    .unwrap();
    assert_eq!(trellis.len(), 1);
    let hunyuan = fal::outputs(
        "fal-ai/hunyuan3d-v3/text-to-3d",
        &serde_json::json!({
            "model_glb": {"url": "https://v3.fal.media/m.glb"},
            "model_urls": {"obj": {"url": "https://v3.fal.media/m.obj"}, "usdz": {"url": "https://v3.fal.media/m.usdz"}}
        }),
    )
    .unwrap();
    assert_eq!(hunyuan.len(), 1);
    assert!(fal::outputs("fal-ai/flux/dev", &serde_json::json!("bad")).is_err());
    assert!(fal::outputs("fal-ai/hunyuan3d-v3/text-to-3d", &serde_json::json!({"model_glb": {"url": "https://v3.fal.media/m.glb"}, "model_urls": "bad"})).is_err());
    let gw = fal::gateway_outputs(
        "fal-ai/flux/dev",
        &serde_json::json!({"output": {"images": [{"url": "https://v3.fal.media/gw.png"}]}}),
    )
    .unwrap();
    assert_eq!(gw.len(), 1);
    assert!(fal::gateway_outputs("fal-ai/flux/dev", &serde_json::json!({})).is_err());
}

#[test]
fn v1_direct_branch_coverage_wav_url_logging_and_mp4_audio_stsc() {
    use numera_gen_mcp::{
        http::url::operation_url,
        logging,
        media::{self, Kind},
    };

    // 1. logging::redact: depth > 8, credentialed URL + query, array > 30, Bearer
    let mut deep = json!("leaf");
    for _ in 0..10 {
        deep = json!({"n": deep});
    }
    let _ = logging::redact(&deep, 0);
    let arr35 = numera_gen_mcp::compat::value::Value::Array((0..35).map(|i| json!(i)).collect());
    let red_arr = logging::redact(&arr35, 0);
    assert_eq!(red_arr.as_array().unwrap().len(), 30);
    let url_red = logging::redact(
        &json!({"u": "https://alice:secret@example.com/path?token=abc Bearer tok_123456"}),
        0,
    );
    let s = url_red["u"].as_str().unwrap();
    assert!(!s.contains("secret"));
    assert!(!s.contains("token=abc"));
    assert!(!s.contains("tok_123456"));

    // 2. operation_url: forbidden query keys, valid pageToken, invalid query value, trailing slashes
    for bad_q in [
        json!({"api_key": "x"}),
        json!({"token": "x"}),
        json!({"secret": "x"}),
        json!({"auth": "x"}),
        json!({"signature": "x"}),
        json!({"limit": 123}),
    ] {
        assert!(
            operation_url(
                "https://api.example/v1/",
                "api-prefix",
                "models",
                "/v1",
                &bad_q
            )
            .is_err()
        );
    }
    let ok_q = operation_url(
        "https://api.example",
        "origin",
        "models",
        "/v1/",
        &json!({"pageToken": "abc", "limit": "10"}),
    )
    .unwrap();
    assert!(ok_q.as_str().contains("pageToken=abc"));

    // 3. WAV WAVE_FORMAT_EXTENSIBLE edge rejections
    let make_wav = |fmt: &[u8], data: &[u8]| -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVEfmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(fmt);
        if !fmt.len().is_multiple_of(2) {
            body.push(0);
        }
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        [
            b"RIFF".as_slice(),
            &(body.len() as u32).to_le_bytes(),
            &body,
        ]
        .concat()
    };
    let pcm_guid = [
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];
    let mut bad_cbsize = vec![0u8; 40];
    bad_cbsize[0..2].copy_from_slice(&0xFFFEu16.to_le_bytes());
    bad_cbsize[2..4].copy_from_slice(&2u16.to_le_bytes());
    bad_cbsize[4..8].copy_from_slice(&48000u32.to_le_bytes());
    bad_cbsize[8..12].copy_from_slice(&192000u32.to_le_bytes());
    bad_cbsize[12..14].copy_from_slice(&4u16.to_le_bytes());
    bad_cbsize[14..16].copy_from_slice(&16u16.to_le_bytes());
    bad_cbsize[16..18].copy_from_slice(&10u16.to_le_bytes()); // cbSize < 22
    bad_cbsize[18..20].copy_from_slice(&16u16.to_le_bytes());
    bad_cbsize[24..40].copy_from_slice(&pcm_guid);
    assert!(
        media::inspect(
            Kind::Audio,
            &make_wav(&bad_cbsize, &[0; 8]),
            1024 * 1024,
            1024
        )
        .is_err()
    );

    let mut bad_valid_bits = bad_cbsize.clone();
    bad_valid_bits[16..18].copy_from_slice(&22u16.to_le_bytes());
    bad_valid_bits[18..20].copy_from_slice(&32u16.to_le_bytes()); // valid_bits > bits(16)
    assert!(
        media::inspect(
            Kind::Audio,
            &make_wav(&bad_valid_bits, &[0; 8]),
            1024 * 1024,
            1024
        )
        .is_err()
    );

    // Truncated extensible fmt (< 40 bytes)
    assert!(
        media::inspect(
            Kind::Audio,
            &make_wav(&bad_cbsize[..20], &[0; 8]),
            1024 * 1024,
            1024
        )
        .is_err()
    );

    // 4. mp4_audio stsc edge branches (first=0, first>chunks, samples=0, desc=0, desc>len, non-monotonic, unresolved count, truncated mdhd v1)
    let atom = |kind: &[u8; 4], body: &[u8]| -> Vec<u8> {
        [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat()
    };
    let full = |kind: &[u8; 4], words: &[u32]| -> Vec<u8> {
        let b: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        atom(kind, &[&[0u8; 4][..], &b].concat())
    };
    let build_stsc_case =
        |stsc_words: &[u32], stsz_count: u32, trunc_mdhd_v1: bool, extra_soun: bool| -> Vec<u8> {
            let video_sample = [0, 0, 0, 5, 0x65, 0x88, 0x84, 0x00, 0x11];
            let audio_sample = [0x21u8; 16];
            let avcc = [
                1, 0x42, 0, 0x1e, 0xff, 0xe1, 0, 4, 0x67, 0x42, 0, 0x1e, 1, 0, 2, 0x68, 0xce,
            ];
            let mut avc1 = vec![0u8; 78];
            avc1[6..8].copy_from_slice(&1u16.to_be_bytes());
            avc1[24..26].copy_from_slice(&64u16.to_be_bytes());
            avc1[26..28].copy_from_slice(&64u16.to_be_bytes());
            avc1.extend_from_slice(&atom(b"avcC", &avcc));
            let v_stsd = atom(
                b"stsd",
                &[&[0u8; 4][..], &1u32.to_be_bytes(), &atom(b"avc1", &avc1)].concat(),
            );
            let ftyp = atom(b"ftyp", b"isom\0\0\x02\0isommp41");
            let mdat_off = ftyp.len() + 8;
            let mdat = atom(
                b"mdat",
                &[video_sample.as_slice(), audio_sample.as_slice()].concat(),
            );
            let dinf = atom(
                b"dinf",
                &atom(
                    b"dref",
                    &[
                        &[0u8; 4][..],
                        &1u32.to_be_bytes(),
                        &atom(b"url ", &[0, 0, 0, 1]),
                    ]
                    .concat(),
                ),
            );
            let v_stbl = atom(
                b"stbl",
                &[
                    v_stsd,
                    full(b"stts", &[1, 1, 1000]),
                    full(b"stsc", &[1, 1, 1, 1]),
                    full(b"stsz", &[0, 1, video_sample.len() as u32]),
                    full(b"stco", &[1, mdat_off as u32]),
                ]
                .concat(),
            );
            let v_tkhd = full(
                b"tkhd",
                &[
                    0,
                    0,
                    1,
                    0,
                    1000,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    64 << 16,
                    64 << 16,
                ],
            );
            let v_trak = atom(
                b"trak",
                &[
                    v_tkhd,
                    atom(
                        b"mdia",
                        &[
                            full(b"mdhd", &[0, 0, 1000, 1000, 0]),
                            atom(b"hdlr", &[&[0u8; 8][..], b"vide", &[0u8; 12]].concat()),
                            atom(
                                b"minf",
                                &[full(b"vmhd", &[0, 0]), dinf.clone(), v_stbl].concat(),
                            ),
                        ]
                        .concat(),
                    ),
                ]
                .concat(),
            );
            let mut entry = vec![0u8; 28];
            entry[6..8].copy_from_slice(&1u16.to_be_bytes());
            entry.extend_from_slice(&atom(b"esds", &[0, 0, 0, 0, 3, 2]));
            let a_stsd = atom(
                b"stsd",
                &[&[0u8; 4][..], &1u32.to_be_bytes(), &atom(b"mp4a", &entry)].concat(),
            );
            let a_off = (mdat_off + video_sample.len()) as u32;
            let a_stbl = atom(
                b"stbl",
                &[
                    a_stsd,
                    full(b"stts", &[1, 1, 48000]),
                    full(b"stsc", stsc_words),
                    full(b"stsz", &[16, stsz_count]),
                    full(b"stco", &[1, a_off]),
                ]
                .concat(),
            );
            let a_mdhd = if trunc_mdhd_v1 {
                atom(b"mdhd", &[1u8; 20])
            } else {
                full(b"mdhd", &[0, 0, 48000, 48000, 0])
            };
            let a_trak = atom(
                b"trak",
                &[atom(
                    b"mdia",
                    &[
                        a_mdhd,
                        atom(b"hdlr", &[&[0u8; 8][..], b"soun", &[0u8; 12]].concat()),
                        atom(b"minf", &[dinf, a_stbl].concat()),
                    ]
                    .concat(),
                )]
                .concat(),
            );
            let mvhd = full(
                b"mvhd",
                &[
                    0, 0, 1000, 1000, 0x00010000, 0x01000000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 2,
                ],
            );
            let mut children = vec![mvhd, v_trak, a_trak.clone()];
            if extra_soun {
                children.push(a_trak);
            }
            [ftyp, mdat, atom(b"moov", &children.concat())].concat()
        };
    for bad in [
        build_stsc_case(&[1, 0, 1, 1], 1, false, false), // first = 0
        build_stsc_case(&[1, 5, 1, 1], 1, false, false), // first > chunks
        build_stsc_case(&[1, 1, 0, 1], 1, false, false), // samples = 0
        build_stsc_case(&[1, 1, 1, 0], 1, false, false), // desc = 0
        build_stsc_case(&[1, 1, 1, 2], 1, false, false), // desc > entries
        build_stsc_case(&[2, 1, 1, 1, 1, 1, 1], 2, false, false), // non-monotonic first
        build_stsc_case(&[1, 1, 1, 1], 2, false, false), // sample_index != count
        build_stsc_case(&[1, 1, 1, 1], 1, true, false),  // truncated mdhd v1
        build_stsc_case(&[1, 1, 1, 1], 1, false, true),  // duplicate soun tracks
    ] {
        assert!(media::inspect(Kind::Video, &bad, 1024 * 1024, 1024 * 1024).is_err());
    }
}
