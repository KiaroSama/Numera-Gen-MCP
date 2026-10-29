mod common;
#[path = "sdk_acceptance/host.rs"]
mod host;
#[path = "sdk_acceptance/provider.rs"]
mod provider;
#[path = "sdk_acceptance/fixtures.rs"]
mod fixtures;
#[path = "sdk_acceptance/catalog.rs"]
mod catalog;
#[path = "sdk_acceptance/configuration.rs"]
mod configuration;
#[path = "sdk_acceptance/reload.rs"]
mod reload;
#[path = "sdk_acceptance/recovery.rs"]
mod recovery;
#[path = "sdk_acceptance/lifecycle.rs"]
mod lifecycle;
#[path = "sdk_acceptance/transport.rs"]
mod transport;
#[path = "sdk_acceptance/terminal.rs"]
mod terminal;
#[path = "sdk_acceptance/races.rs"]
mod races;
#[path = "sdk_acceptance/http.rs"]
mod http;
#[path = "sdk_acceptance/contracts.rs"]
mod contracts;

// These are real rmcp clients connected to isolated executables. The seed Store is a separate
// owner; existing same-Application tests remain independent and are not replaced by this target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_all_ten_tools_and_exact_shared_two_host_request() {
    catalog::tools().await;
    catalog::two_hosts().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_compact_defaults_adjacent_discovery_and_private_dotenv_reload() {
    configuration::compact().await;
    configuration::dotenv().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_three_protocol_eras_and_failed_handshake_ownership() {
    configuration::negotiation().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_three_saved_configuration_reload_schedules() {
    reload::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_recovery_controls_two_hosts_and_two_actual_restarts() {
    recovery::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_eighteen_literal_terminal_lifecycle_scenarios() {
    lifecycle::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_thirteen_transport_boundary_scenarios() {
    transport::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_terminal_rounds_preserve_authoritative_outcomes_and_capacity() {
    terminal::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_sdk_seven_actual_two_host_races_and_restarts() {
    races::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_http_two_real_sdk_sessions_auth_origin_privacy_and_graceful_exit() {
    http::run().await;
}
