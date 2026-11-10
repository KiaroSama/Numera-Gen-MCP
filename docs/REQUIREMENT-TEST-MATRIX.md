# Requirement-to-test matrix

The maintained implementation, tests and CI driver are Rust. The immutable baseline keeps all
50 suites and 470 case identities: 178 unit, 132 integration and 160 contract. The reference
branch's 688 Rust test declarations remain present. Cargo discovers workspace harnesses;
[the parity inventory](testing/native-parity.json) binds original case identities separately.

Paths below are current Rust sources. A listed assertion is not an execution claim: use the
exact commit's automatic CI and [test results](TEST-RESULTS.md) for measured evidence.

| Area | Current assertions / evidence |
| --- | --- |
| Config precedence, isolation, origin and authentication | `tests/rust/config_contracts.rs`, `config_acceptance.rs`, `config_parity.rs` |
| URL prefixes, traversal, queries, DNS and asset destinations | `tests/rust/api_contracts.rs`, `network_acceptance.rs`, `asset_dns_acceptance.rs` |
| Catalog pagination, caching, partial results and gateway differences | `tests/rust/discovery*.rs`, `catalog_upload_contracts.rs` |
| Capabilities, defaults, model limits, conflicts and unsupported settings | `tests/rust/model_policy.rs`, `policy_acceptance.rs`, `reference_requirements_acceptance.rs` |
| Native adapter method, path, headers, body and exact model/prompt forwarding | `tests/rust/adapters.rs`, `provider_wire_contracts.rs`, `generation_contracts.rs` |
| Multipart/JSON references, masks, translation and native editing | `tests/rust/multipart.rs`, `reference_preflight_acceptance.rs`, `image_editing_acceptance.rs` |
| Version-scoped 9router reference forwarding and cardinality | `tests/rust/image_route_contracts.rs`, `examples/9router-current.json`, [upstream review](REFERENCES.md) |
| Binary, Responses, Interactions and bounded SSE/error/preview handling | `tests/rust/streams.rs`, `streams_acceptance.rs`, `normalization_acceptance.rs` |
| PNG/JPEG/WebP originals, Base64 limits, pixels, alpha, masks and paths | `tests/rust/images.rs`, `editing.rs`, `output_requirements.rs`, `native/files/paths_races.rs` |
| Atomic outputs, partial success, deviations, hashes and publication faults | `tests/rust/publication_contracts.rs`, `publication_recovery_acceptance.rs`, `persistence_fault_acceptance.rs` |
| Request identity, shared state, leases, admission, cancellation and restart | `tests/rust/job_recovery_contracts.rs`, `lease_clock_acceptance.rs`, `generation_admission.rs`, `terminal_lifecycle_acceptance.rs` |
| No paid POST replay and retained originals after uncertain outcomes | `tests/rust/sdk_acceptance.rs`, `detached_job_acceptance.rs`, `local_recovery_acceptance.rs` |
| Actual SDK stdio/HTTP handshake, tools, resources, protocol eras and cleanup | `tests/rust/sdk_acceptance.rs`, `http_cleanup_acceptance.rs`, `transport_acceptance.rs` |
| Compact configuration, foreign CWD, saved reloads and pinned credentials | `tests/rust/env_acceptance.rs`, `environment_snapshot_acceptance.rs`, `snapshot_races.rs` |
| Read-only readiness and explicit host-registration transactions | `tests/rust/readiness_acceptance.rs`, `registration_contracts.rs`, `registration_acceptance.rs` |
| POSIX private files, Unicode paths, symlinks and owned process groups | `tests/rust/posix_acceptance.rs`, `devtool_supervisor.rs`, `native/devtools/linux_snapshot_tests.rs` |
| Windows native DPAPI, private ACLs, Unicode and owned worker abort | `tests/rust/dpapi_acceptance.rs`, `devtool_credentials.rs`, `common/dpapi_worker.rs` |
| Comfy workflow bindings, upload/history/view, targeted cancellation and races | `tests/rust/comfy*.rs`, `workflow.rs` |
| Typed image/video/audio/3D requests, endpoint contracts and legacy image compatibility | `tests/rust/media_config_contracts.rs`, `media_fal_contracts.rs`, `media_acceptance.rs` |
| WAV, MP4 and GLB structural validation, limits and malformed originals | `tests/rust/media_format_contracts.rs`, `media_format_cases/` |
| Owned media output metadata, original bytes and SHA256 | `tests/rust/media_output_contracts.rs`, `media_acceptance.rs` |
| Release packaging, checksums, licenses, extraction and actual extracted SDK | `tooling/tests/distribution_contracts.rs`, `archive_contracts.rs`, `licenses_contracts.rs`; required six-target CI phases |
| Existing 51 tooling migrations and original hash/scenario continuity | `docs/testing/tooling-migration.json`, `tooling/tests/tooling_execution_contracts.rs`, `source_review_contracts.rs` |
| Native signatures and wrong-signer/tamper refusal | `tests/rust/artifact_signatures.rs`, `identity_contracts.rs`, `tooling/tests/signature_acceptance_contracts.rs`; protected manual signed qualification |
| Live providers, interactive GUI/input, other-user DPAPI denial and installed GPU hosts | Separate real inputs and authorization; no claim from synthetic fixtures |

All six Windows/Linux/macOS x64/ARM64 CI jobs require formatting, Clippy, existing tests, a release
build, original dependency notices and extracted-binary SDK execution. The coverage job executes
existing instrumented workspace tests and reports production code against unchanged 85% region /
80% branch floors. Stable native execution records formal tooling scenario bindings separately.
Signing and release publication remain protected manual operations.

Synthetic images and structural media fixtures establish original bytes, container metadata,
bounded parsing and protocol behavior. They do not establish model quality, image translation
fidelity, video playback, an actual gateway process, host GUI reconnection or GPU generation.
Historical TypeScript paths remain only in immutable baseline records and historical evidence;
they are not maintained commands or CI dependencies.
