#[allow(dead_code)]
#[path = "sdk_acceptance/cleanup.rs"]
mod cleanup;
mod common;
#[path = "media_acceptance/failures.rs"]
mod failures;
#[allow(dead_code)]
#[path = "sdk_acceptance/fixtures.rs"]
mod fixtures;
#[allow(dead_code)]
#[path = "sdk_acceptance/host.rs"]
mod host;
#[path = "media_acceptance/legacy.rs"]
mod legacy;
#[path = "media_acceptance/lifecycle.rs"]
mod lifecycle;
#[path = "../fixtures/media/bytes.rs"]
mod media_bytes;
#[path = "media_acceptance/modes.rs"]
mod modes;
#[path = "media_acceptance/preflight.rs"]
mod preflight;
#[allow(dead_code)]
#[path = "sdk_acceptance/provider.rs"]
mod provider;
#[path = "media_acceptance/support.rs"]
mod support;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sdk_media_kinds_modes_outputs_and_legacy_catalog() {
    modes::run().await;
    legacy::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sdk_media_existing_handle_restart_and_concurrent_identity() {
    lifecycle::recovery_and_concurrency().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sdk_media_unknown_acknowledgement_never_replays() {
    lifecycle::unknown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sdk_media_targeted_cancellation_races_and_refund_distinction() {
    lifecycle::cancellation().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sdk_media_anonymous_assets_partial_outputs_and_validation_failures() {
    failures::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_preflight_defaults_deadlines_and_completed_originals() {
    preflight::run().await;
}
