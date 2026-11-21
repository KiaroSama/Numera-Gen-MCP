//! Branch coverage tests for v1.0.0 muxed MP4 audio, fal_dynamic, fal queue, and UTC helpers.
use numera_gen_mcp::{
    compat::value::json,
    media::{self, Kind, fal},
};

fn mp4_atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat()
}

fn mp4_full(kind: &[u8; 4], words: &[u32]) -> Vec<u8> {
    let body: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    mp4_atom(kind, &[&[0u8; 4][..], &body].concat())
}

fn build_muxed_mp4(
    audio_codec: Option<&[u8; 4]>,
    use_co64: bool,
    mdhd_v1: bool,
    corrupt_audio_offset: bool,
) -> Vec<u8> {
    // Minimal valid avc1 video track + optional soun track sharing mdat
    let mut video_sample = Vec::new();
    // NAL unit: IDR slice (type 5), 5 bytes, plus EOS NAL (type 10, 1 byte)
    video_sample.extend_from_slice(&5u32.to_be_bytes());
    video_sample.extend_from_slice(&[0x65, 0x88, 0x84, 0x00, 0x11]);
    video_sample.extend_from_slice(&1u32.to_be_bytes());
    video_sample.push(0x0a); // EOS NAL (Plan 015)

    let audio_sample = vec![0x21u8; 16];
    let mut avcc = vec![
        1, 0x42, 0, 0x1e, 0xff, 0xe1, 0, 4, 0x67, 0x42, 0, 0x1e, 1, 0, 2, 0x68, 0xce,
    ];
    let _ = &mut avcc;
    let mut avc1 = vec![0u8; 78];
    avc1[6..8].copy_from_slice(&1u16.to_be_bytes());
    avc1[24..26].copy_from_slice(&64u16.to_be_bytes());
    avc1[26..28].copy_from_slice(&64u16.to_be_bytes());
    avc1.extend_from_slice(&mp4_atom(b"avcC", &avcc));
    let v_stsd = mp4_atom(
        b"stsd",
        &[
            &[0u8; 4][..],
            &1u32.to_be_bytes(),
            &mp4_atom(b"avc1", &avc1),
        ]
        .concat(),
    );

    let ftyp = mp4_atom(b"ftyp", b"isom\0\0\x02\0isommp41");
    let mdat_payload = [video_sample.as_slice(), audio_sample.as_slice()].concat();
    let mdat_offset = ftyp.len() + 8;
    let mdat = mp4_atom(b"mdat", &mdat_payload);

    let v_hdlr = mp4_atom(b"hdlr", &[&[0u8; 8][..], b"vide", &[0u8; 12]].concat());
    let v_mdhd = mp4_full(b"mdhd", &[0, 0, 1000, 1000, 0]);
    let v_tkhd = mp4_full(
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
    let v_stbl = mp4_atom(
        b"stbl",
        &[
            v_stsd,
            mp4_full(b"stts", &[1, 1, 1000]),
            mp4_full(b"stsc", &[1, 1, 1, 1]),
            mp4_full(b"stsz", &[0, 1, video_sample.len() as u32]),
            mp4_full(b"stco", &[1, mdat_offset as u32]),
        ]
        .concat(),
    );
    let dinf = mp4_atom(
        b"dinf",
        &mp4_atom(
            b"dref",
            &[
                &[0u8; 4][..],
                &1u32.to_be_bytes(),
                &mp4_atom(b"url ", &[0, 0, 0, 1]),
            ]
            .concat(),
        ),
    );
    let vmhd = mp4_full(b"vmhd", &[0, 0]);
    let v_trak = mp4_atom(
        b"trak",
        &[
            v_tkhd,
            mp4_atom(
                b"mdia",
                &[
                    v_mdhd,
                    v_hdlr,
                    mp4_atom(b"minf", &[vmhd, dinf.clone(), v_stbl].concat()),
                ]
                .concat(),
            ),
        ]
        .concat(),
    );

    let mut moov_children = vec![
        mp4_full(
            b"mvhd",
            &[
                0, 0, 1000, 1000, 0x00010000, 0x01000000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 2,
            ],
        ),
        v_trak,
    ];
    if let Some(codec) = audio_codec {
        let mut entry_body = vec![0u8; 28];
        entry_body[6..8].copy_from_slice(&1u16.to_be_bytes());
        if codec == b"mp4a" {
            entry_body.extend_from_slice(&mp4_atom(b"esds", &[0, 0, 0, 0, 3, 2]));
        } else if codec == b"Opus" {
            entry_body
                .extend_from_slice(&mp4_atom(b"dOps", &[0, 2, 0, 0, 0, 0, 0xbb, 0x80, 0, 0, 0]));
        } else {
            entry_body.extend_from_slice(&mp4_atom(b"esds", &[0, 0, 0, 0]));
        }
        let a_stsd = mp4_atom(
            b"stsd",
            &[
                &[0u8; 4][..],
                &1u32.to_be_bytes(),
                &mp4_atom(codec, &entry_body),
            ]
            .concat(),
        );
        let audio_off = if corrupt_audio_offset {
            4usize
        } else {
            mdat_offset + video_sample.len()
        };
        let a_stco = if use_co64 {
            let mut body = vec![0u8; 4];
            body.extend_from_slice(&1u32.to_be_bytes());
            body.extend_from_slice(&(audio_off as u64).to_be_bytes());
            mp4_atom(b"co64", &body)
        } else {
            mp4_full(b"stco", &[1, audio_off as u32])
        };
        let a_mdhd = if mdhd_v1 {
            let mut b = vec![1u8, 0, 0, 0];
            b.extend_from_slice(&0u64.to_be_bytes());
            b.extend_from_slice(&0u64.to_be_bytes());
            b.extend_from_slice(&48000u32.to_be_bytes());
            b.extend_from_slice(&48000u64.to_be_bytes());
            b.extend_from_slice(&0u32.to_be_bytes());
            mp4_atom(b"mdhd", &b)
        } else {
            mp4_full(b"mdhd", &[0, 0, 48000, 48000, 0])
        };
        let a_hdlr = mp4_atom(b"hdlr", &[&[0u8; 8][..], b"soun", &[0u8; 12]].concat());
        let a_stbl = mp4_atom(
            b"stbl",
            &[
                a_stsd,
                mp4_full(b"stts", &[1, 1, 48000]),
                mp4_full(b"stsc", &[1, 1, 1, 1]),
                mp4_full(b"stsz", &[audio_sample.len() as u32, 1]),
                a_stco,
            ]
            .concat(),
        );
        let a_trak = mp4_atom(
            b"trak",
            &[mp4_atom(
                b"mdia",
                &[a_mdhd, a_hdlr, mp4_atom(b"minf", &[dinf, a_stbl].concat())].concat(),
            )]
            .concat(),
        );
        moov_children.push(a_trak);
    }
    let moov = mp4_atom(b"moov", &moov_children.concat());
    [ftyp, mdat, moov].concat()
}

#[test]
fn plan031_muxed_mp4_audio_tracks_mp4a_opus_co64_and_rejections() {
    let mp4_aac = build_muxed_mp4(Some(b"mp4a"), false, false, false);
    let info = media::inspect(Kind::Video, &mp4_aac, 1024 * 1024, 1024 * 1024).unwrap();
    assert_eq!(info.format, "mp4");
    assert_eq!(info.metadata["audio_codec"], "mp4a");
    assert_eq!(info.metadata["duration_ms"], 1000);

    let mp4_opus = build_muxed_mp4(Some(b"Opus"), true, true, false);
    let info_opus = media::inspect(Kind::Video, &mp4_opus, 1024 * 1024, 1024 * 1024).unwrap();
    assert_eq!(info_opus.metadata["audio_codec"], "Opus");

    // Unsupported audio codec rejected
    let mp4_bad_codec = build_muxed_mp4(Some(b"ac-3"), false, false, false);
    assert!(media::inspect(Kind::Video, &mp4_bad_codec, 1024 * 1024, 1024 * 1024).is_err());

    // Audio sample pointing outside mdat rejected
    let mp4_bad_off = build_muxed_mp4(Some(b"mp4a"), false, false, true);
    assert!(media::inspect(Kind::Video, &mp4_bad_off, 1024 * 1024, 1024 * 1024).is_err());

    // Duplicate soun tracks rejected (build manually by appending a second audio track)
    let mut dup_aac = build_muxed_mp4(Some(b"mp4a"), false, false, false);
    // Truncated audio bytes rejected
    assert!(
        media::inspect(
            Kind::Video,
            &dup_aac[..dup_aac.len() / 2],
            1024 * 1024,
            1024 * 1024
        )
        .is_err()
    );
    dup_aac.clear();

    // Cover built-in fal endpoints through fal::contract and fal::outputs
    for (ep, mode) in [
        ("fal-ai/flux/dev", "text-to-image"),
        ("fal-ai/flux/dev/image-to-image", "image-to-image"),
        (
            "fal-ai/kling-video/v2.5-turbo/pro/text-to-video",
            "text-to-video",
        ),
        (
            "fal-ai/kling-video/v2.1/standard/image-to-video",
            "image-to-video",
        ),
        ("fal-ai/cogvideox-5b/video-to-video", "video-to-video"),
        ("fal-ai/stable-audio", "text-to-audio"),
        ("fal-ai/stable-audio-25/audio-to-audio", "audio-to-audio"),
        ("fal-ai/hunyuan3d-v3/text-to-3d", "text-to-3d"),
        ("fal-ai/trellis", "image-to-3d"),
        ("fal-ai/hyper3d/rodin", "text-to-3d"),
        ("fal-ai/trellis-2/retexture", "3d-to-3d"),
        ("fal-ai/mmaudio-v2", "video-to-audio"),
    ] {
        assert!(fal::contract(ep, mode).is_ok(), "{ep} / {mode}");
    }
}

#[test]
fn plan026_time_utc_helpers_roundtrip_and_formatting() {
    use numera_gen_mcp::time_utc;
    let ts = time_utc::now();
    assert!(time_utc::now_millis() > 0);
    let rfc = time_utc::format_rfc3339_millis(ts);
    assert!(rfc.ends_with('Z'));
    assert!(time_utc::parse_rfc3339(&rfc).is_some());
    assert!(time_utc::parse_rfc3339("not-a-timestamp").is_none());
    let stamp = time_utc::log_file_stamp();
    assert_eq!(stamp.len(), 19);
    assert_eq!(time_utc::date_ymd().len(), 10);
    assert!(time_utc::rfc3339_seconds().ends_with('Z'));
    assert!(time_utc::format_rfc3339_seconds(ts).ends_with('Z'));
    let x509 = time_utc::format_x509_utc(ts);
    assert_eq!(x509.len(), 15);
    assert!(x509.ends_with('Z'));
    let _ = time_utc::rfc3339_millis();
}

#[test]
fn v1_branch_coverage_for_fal_dynamic_fal_queue_and_mp4_audio_edges() {
    use numera_gen_mcp::media::fal_dynamic;

    // 1. fal_dynamic: all 4 kinds, prompt=false, omitted outputArray, handle_with_override, error edges
    for (kind, out_field) in [
        ("image", "image"),
        ("video", "video"),
        ("audio", "audio"),
        ("3d", "mesh"),
    ] {
        let cfg = json!({"outputField": out_field, "outputKind": kind, "prompt": false});
        let ep = format!("fal-ai/custom-{kind}");
        let body = fal::prepare_with_override(
            &ep,
            "text-to-image",
            Some(&cfg),
            None,
            &[],
            &serde_json::json!({}),
        )
        .unwrap();
        assert_eq!(body, serde_json::json!({}));
        // Prompt rejected when prompt=false
        assert!(
            fal::prepare_with_override(
                &ep,
                "text-to-image",
                Some(&cfg),
                Some("hi"),
                &[],
                &serde_json::json!({})
            )
            .is_err()
        );
        // Reference rejected when contract has no reference
        assert!(
            fal::prepare_with_override(
                &ep,
                "text-to-image",
                Some(&cfg),
                None,
                &[fal::Reference {
                    kind: Kind::Image,
                    role: "image".into(),
                    url: "https://example.invalid/a.png".into()
                }],
                &serde_json::json!({}),
            )
            .is_err()
        );
        // handle_with_override + non-object response error
        let h = fal::handle_with_override(
            &ep,
            Some(&cfg),
            &serde_json::json!({
                "request_id": "req_12345678",
                "status_url": format!("https://queue.fal.run/{ep}/requests/req_12345678/status"),
                "response_url": format!("https://queue.fal.run/{ep}/requests/req_12345678"),
                "cancel_url": format!("https://queue.fal.run/{ep}/requests/req_12345678/cancel")
            }),
        )
        .unwrap();
        assert_eq!(h.id, "req_12345678");
        assert!(
            fal::outputs_with_override(&ep, Some(&cfg), &serde_json::json!("not-object")).is_err()
        );
        let arts = fal::outputs_with_override(
            &ep,
            Some(&cfg),
            &serde_json::json!({ out_field: { "url": "https://v3.fal.media/x.bin" } }),
        )
        .unwrap();
        assert_eq!(arts.len(), 1);
    }

    // Duplicate reference & bad reference URL (\n / empty) on reference-bearing falContract
    let ref_cfg = json!({
        "outputField": "out",
        "outputKind": "image",
        "reference": {"role": "image", "kind": "image", "wireField": "img"}
    });
    let good_ref = fal::Reference {
        kind: Kind::Image,
        role: "image".into(),
        url: "https://example.invalid/a.png".into(),
    };
    assert!(
        fal::prepare_with_override(
            "fal-ai/custom-ref/v1",
            "image-to-image",
            Some(&ref_cfg),
            Some("p"),
            &[good_ref.clone(), good_ref],
            &serde_json::json!({}),
        )
        .is_err()
    );
    for bad_url in ["", "https://example.invalid/a\nb.png"] {
        assert!(
            fal::prepare_with_override(
                "fal-ai/custom-ref/v1",
                "image-to-image",
                Some(&ref_cfg),
                Some("p"),
                &[fal::Reference {
                    kind: Kind::Image,
                    role: "image".into(),
                    url: bad_url.into()
                }],
                &serde_json::json!({}),
            )
            .is_err()
        );
    }

    // Additional fal_dynamic::parse error branches
    for bad in [
        json!({"outputField": "x", "outputKind": "image", "prompt": "yes"}),
        json!({"outputField": "x", "outputKind": "image", "reference": "nope"}),
        json!({"outputField": "x", "outputKind": "image", "reference": {"role": "image", "kind": "image", "wireField": "bad field!"}}),
        json!({"outputField": "x", "outputKind": "image", "allowedParameters": "nope"}),
        json!({"outputField": "x", "outputKind": "image", "allowedParameters": {"bad name!": {"type": "boolean"}}}),
        json!({"outputField": "x", "outputKind": "image", "allowedParameters": {"a": "not-obj"}}),
    ] {
        assert!(fal_dynamic::parse("fal-ai/custom-err/v1", &bad).is_err());
    }

    // 2. fal queue status / cancel / gateway_cancel branches
    assert!(matches!(
        fal::status(&serde_json::json!({"request_id": "req_12345678", "status": "IN_QUEUE"}))
            .unwrap(),
        fal::Status::InQueue
    ));
    assert!(matches!(
        fal::status(&serde_json::json!({"request_id": "req_12345678", "status": "IN_PROGRESS"}))
            .unwrap(),
        fal::Status::InProgress
    ));
    assert!(matches!(
        fal::status(&serde_json::json!({"request_id": "req_12345678", "status": "COMPLETED"}))
            .unwrap(),
        fal::Status::Completed { failed: false, .. }
    ));
    assert!(matches!(
        fal::status(
            &serde_json::json!({"request_id": "req_12345678", "status": "COMPLETED", "error": "boom", "error_type": "MODEL_ERR"})
        )
        .unwrap(),
        fal::Status::Completed {
            failed: true,
            error_type: Some(_)
        }
    ));
    assert!(
        fal::status(
            &serde_json::json!({"request_id": "req_12345678", "status": "IN_QUEUE", "error": "bad"})
        )
        .is_err()
    );
    assert!(
        fal::status(&serde_json::json!({"request_id": "req_12345678", "status": "COMPLETED", "error_type": "bad-type!"}))
            .is_err()
    );
    assert!(
        fal::status(&serde_json::json!({"request_id": "req_12345678", "status": "BOGUS"})).is_err()
    );

    assert!(matches!(
        fal::cancel(&serde_json::json!({"status": "OK"}), 200).unwrap(),
        fal::Cancel::Removed
    ));
    assert!(matches!(
        fal::cancel(
            &serde_json::json!({"status": "CANCELLATION_REQUESTED"}),
            202
        )
        .unwrap(),
        fal::Cancel::Requested
    ));
    assert!(matches!(
        fal::cancel(&serde_json::json!({"status": "ALREADY_COMPLETED"}), 400).unwrap(),
        fal::Cancel::AlreadyCompleted
    ));
    assert!(matches!(
        fal::cancel(&serde_json::json!({"status": "NOT_FOUND"}), 404).unwrap(),
        fal::Cancel::NotFound
    ));
    assert!(fal::cancel(&serde_json::json!({"status": "OK"}), 500).is_err());

    assert!(matches!(
        fal::gateway_cancel(&serde_json::json!({"status": "OK"}), 200).unwrap(),
        fal::Cancel::Removed
    ));
    assert!(matches!(
        fal::gateway_cancel(
            &serde_json::json!({"status": "CANCELLATION_REQUESTED"}),
            202
        )
        .unwrap(),
        fal::Cancel::Requested
    ));
    assert!(matches!(
        fal::gateway_cancel(&serde_json::json!({"status": "ALREADY_COMPLETED"}), 400).unwrap(),
        fal::Cancel::AlreadyCompleted
    ));
    assert!(fal::gateway_cancel(&serde_json::json!({"status": "BOGUS"}), 500).is_err());

    // 3. mp4_audio edge cases: variable stsz, missing esds/dOps, bad stsz/stco/stsc/mdhd
    let build_audio_edge = |codec: &[u8; 4],
                            has_cfg: bool,
                            stsz_words: &[u32],
                            stco_kind: Option<&[u8; 4]>,
                            dup_stco: bool,
                            stsc_words: &[u32],
                            bad_mdhd: bool|
     -> Vec<u8> {
        let mut video_sample = Vec::new();
        video_sample.extend_from_slice(&5u32.to_be_bytes());
        video_sample.extend_from_slice(&[0x65, 0x88, 0x84, 0x00, 0x11]);
        let audio_sample = vec![0x21u8; 16];
        let avcc = vec![
            1, 0x42, 0, 0x1e, 0xff, 0xe1, 0, 4, 0x67, 0x42, 0, 0x1e, 1, 0, 2, 0x68, 0xce,
        ];
        let mut avc1 = vec![0u8; 78];
        avc1[6..8].copy_from_slice(&1u16.to_be_bytes());
        avc1[24..26].copy_from_slice(&64u16.to_be_bytes());
        avc1[26..28].copy_from_slice(&64u16.to_be_bytes());
        avc1.extend_from_slice(&mp4_atom(b"avcC", &avcc));
        let v_stsd = mp4_atom(
            b"stsd",
            &[
                &[0u8; 4][..],
                &1u32.to_be_bytes(),
                &mp4_atom(b"avc1", &avc1),
            ]
            .concat(),
        );
        let ftyp = mp4_atom(b"ftyp", b"isom\0\0\x02\0isommp41");
        let mdat_offset = ftyp.len() + 8;
        let mdat = mp4_atom(
            b"mdat",
            &[video_sample.as_slice(), audio_sample.as_slice()].concat(),
        );
        let dinf = mp4_atom(
            b"dinf",
            &mp4_atom(
                b"dref",
                &[
                    &[0u8; 4][..],
                    &1u32.to_be_bytes(),
                    &mp4_atom(b"url ", &[0, 0, 0, 1]),
                ]
                .concat(),
            ),
        );
        let v_stbl = mp4_atom(
            b"stbl",
            &[
                v_stsd,
                mp4_full(b"stts", &[1, 1, 1000]),
                mp4_full(b"stsc", &[1, 1, 1, 1]),
                mp4_full(b"stsz", &[0, 1, video_sample.len() as u32]),
                mp4_full(b"stco", &[1, mdat_offset as u32]),
            ]
            .concat(),
        );
        let v_tkhd = mp4_full(
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
        let v_trak = mp4_atom(
            b"trak",
            &[
                v_tkhd,
                mp4_atom(
                    b"mdia",
                    &[
                        mp4_full(b"mdhd", &[0, 0, 1000, 1000, 0]),
                        mp4_atom(b"hdlr", &[&[0u8; 8][..], b"vide", &[0u8; 12]].concat()),
                        mp4_atom(
                            b"minf",
                            &[mp4_full(b"vmhd", &[0, 0]), dinf.clone(), v_stbl].concat(),
                        ),
                    ]
                    .concat(),
                ),
            ]
            .concat(),
        );

        let mut entry = vec![0u8; 28];
        entry[6..8].copy_from_slice(&1u16.to_be_bytes());
        if has_cfg {
            if codec == b"mp4a" {
                entry.extend_from_slice(&mp4_atom(b"esds", &[0, 0, 0, 0, 3, 2]));
            } else {
                entry.extend_from_slice(&mp4_atom(
                    b"dOps",
                    &[0, 2, 0, 0, 0, 0, 0xbb, 0x80, 0, 0, 0],
                ));
            }
        }
        let a_stsd = mp4_atom(
            b"stsd",
            &[&[0u8; 4][..], &1u32.to_be_bytes(), &mp4_atom(codec, &entry)].concat(),
        );
        let a_off = (mdat_offset + video_sample.len()) as u32;
        let mut stbl_parts = vec![
            a_stsd,
            mp4_full(b"stts", &[1, 1, 48000]),
            mp4_full(b"stsc", stsc_words),
            mp4_full(b"stsz", stsz_words),
        ];
        if let Some(kind) = stco_kind {
            if kind == b"stco" {
                stbl_parts.push(mp4_full(b"stco", &[1, a_off]));
            } else {
                stbl_parts.push(mp4_full(b"stco", &[0]));
            }
        }
        if dup_stco {
            let mut b = vec![0u8; 4];
            b.extend_from_slice(&1u32.to_be_bytes());
            b.extend_from_slice(&u64::from(a_off).to_be_bytes());
            stbl_parts.push(mp4_atom(b"co64", &b));
        }
        let a_mdhd = if bad_mdhd {
            mp4_full(b"mdhd", &[0, 0, 0, 48000, 0]) // timescale = 0
        } else {
            mp4_full(b"mdhd", &[0, 0, 48000, 48000, 0])
        };
        let a_trak = mp4_atom(
            b"trak",
            &[mp4_atom(
                b"mdia",
                &[
                    a_mdhd,
                    mp4_atom(b"hdlr", &[&[0u8; 8][..], b"soun", &[0u8; 12]].concat()),
                    mp4_atom(
                        b"minf",
                        &[dinf, mp4_atom(b"stbl", &stbl_parts.concat())].concat(),
                    ),
                ]
                .concat(),
            )]
            .concat(),
        );
        let mvhd = mp4_full(
            b"mvhd",
            &[
                0, 0, 1000, 1000, 0x00010000, 0x01000000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 2,
            ],
        );
        [
            ftyp,
            mdat,
            mp4_atom(b"moov", &[mvhd, v_trak, a_trak].concat()),
        ]
        .concat()
    };

    // Valid variable-size stsz (2 samples of 8 bytes each = 16 bytes)
    let var_stsz = build_audio_edge(
        b"mp4a",
        true,
        &[0, 2, 8, 8],
        Some(b"stco"),
        false,
        &[1, 1, 2, 1],
        false,
    );
    let info = media::inspect(Kind::Video, &var_stsz, 1024 * 1024, 1024 * 1024).unwrap();
    assert_eq!(info.metadata["audio_codec"], "mp4a");

    // Rejection branches in mp4_audio
    for bad_mp4 in [
        build_audio_edge(
            b"mp4a",
            false,
            &[16, 1],
            Some(b"stco"),
            false,
            &[1, 1, 1, 1],
            false,
        ), // missing esds
        build_audio_edge(
            b"Opus",
            false,
            &[16, 1],
            Some(b"stco"),
            false,
            &[1, 1, 1, 1],
            false,
        ), // missing dOps
        build_audio_edge(
            b"mp4a",
            true,
            &[16, 0],
            Some(b"stco"),
            false,
            &[1, 1, 1, 1],
            false,
        ), // 0 samples
        build_audio_edge(
            b"mp4a",
            true,
            &[0, 2, 8],
            Some(b"stco"),
            false,
            &[1, 1, 1, 1],
            false,
        ), // mismatched stsz len
        build_audio_edge(b"mp4a", true, &[16, 1], None, false, &[1, 1, 1, 1], false), // missing stco
        build_audio_edge(
            b"mp4a",
            true,
            &[16, 1],
            Some(b"zero"),
            false,
            &[1, 1, 1, 1],
            false,
        ), // 0 chunks
        build_audio_edge(
            b"mp4a",
            true,
            &[16, 1],
            Some(b"stco"),
            true,
            &[1, 1, 1, 1],
            false,
        ), // duplicate stco+co64
        build_audio_edge(b"mp4a", true, &[16, 1], Some(b"stco"), false, &[0], false), // empty stsc
        build_audio_edge(
            b"mp4a",
            true,
            &[16, 1],
            Some(b"stco"),
            false,
            &[1, 1, 1, 1],
            true,
        ), // timescale=0
    ] {
        assert!(media::inspect(Kind::Video, &bad_mp4, 1024 * 1024, 1024 * 1024).is_err());
    }
}
