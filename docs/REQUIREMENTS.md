# Implementation requirements

Owner assignment dated 2026-10-02; this is a faithful public requirements summary, not a copy of private working material.

## Product

Personal provider-independent native Rust generation/transformation MCP server, **Numera Gen MCP**,
executable/package `numera-gen-mcp`, registration `numera-gen`, GPL-3.0-only. Preserve legacy image
generation/editing while adding explicitly declared image/video/audio/3D modes. Keep the host chat
model unchanged; configured modalities never establish forwarding or account entitlement.
Target repository https://github.com/KiaroSama/Numera-Gen-MCP. Example checkout location: `C:/projects/numera-gen-mcp`; use your own absolute paths.

## Native distribution requirements (2026-10-11)

Preserve the full existing tool/config/provider/state/original-byte behavior while replacing the
production Node implementation. Hosted CI builds ready Windows/Linux/macOS x86_64/ARM64 archives,
SHA-256 checksums and license notices; normal installation requires download/extract/config only,
never a local build or Node/Rust runtime installation. Native `bridge` is launched directly by the
MCP host with `args: ["bridge"]`; no manual launcher startup is required. All managed temp/cache stays
inside the project/install root. Old local runtime/dependencies are removed only after owner config
cutover and verified native reconnection. Private config, originals, journals, snapshots and Git
history are preserved. All470 baseline cases have reviewed applicable native execution; retirement
tree `f253557` passed run `40244894089`, attempt2, on all six targets, including real native signatures
and signed maintenance. Verified Release0.2.0 is published; actual owner cutover remains separate.
See the acceptance matrix.

## Required release contracts

The October 28 scope adds paired per-model output types and modes, strict compact/env equivalents,
exact evidenced fal queue routes and typed image/MP4/WAV/GLB originals. `generate_media` and
`get_media_output_info` are additive; existing jobs/cancellation/output listing remain shared.
A transformation is available only for its implemented exact endpoint and required input roles;
unsupported modes, missing references and unknown outcomes never fall back or resubmit. Hosted
qualification of this source remains distinct from the historical 0.2.0 results below.

Generic Images-compatible generation/multipart/configured JSON edits; independent OmniRoute and
9router gateway profiles; Gemini generateContent; separate Gemini Interactions; OpenAI Responses
with separate orchestration/image tool models; dedicated OpenRouter Image API; explicit opt-in
chat-style image output; directly configured ComfyUI API-format workflow execution.

Ten retained image tools: health_check, list_connections, list_models, get_model_capabilities,
generate_image, edit_image, get_job, cancel_job, list_outputs, get_output_info. Typed media adds the
two tools above. Stdio production transport and optional authenticated loopback HTTP share the same
application services. stdout contains protocol only in stdio mode. Prompt/source fidelity, all valid finals saved, original
bytes/hash/metadata checked, bounded optional derivative previews and owned resources.

## Safety and operational scope

Versioned schema/config, namespaced precedence, isolated credential sources, origin binding, DPAPI
option and portable private files/env. Never request keys in tools. Resolve prefixes once; bounded
read-only discovery/pagination. Capability/model/forwarding/account/evidence separate. Native batches
only, no hidden paid loops, unsupported refs/masks/options rejected before submission.

Persist request intent before submission; fingerprint/identity conflict detection and shared-process
coordination; no paid POST retry or uncertain replay/provider fallback. Cancellation and upstream
billing distinct. Path/realpath/byte/pixel/aggregate/reference limits; public asset DNS/redirect checks,
no credential forwarding or hidden upload service. Atomic output writes, private state/logs.

## Deliverables and evidence

Stable Rust native binaries with locked dependencies, licensing/notices, schemas/examples, separate
native maintenance with five dry-runs and selected-entry backup/concurrency protection; platform and
Claude/Codex docs. Hosted six-target native unit/contract/real-client/extracted archive checks and
85 percent executable-region/80 percent branch coverage without broad exclusions. The frozen Node24
baseline is a CI-only compatibility oracle. A signed GitHub Release is authorized only after all gates;
paid generation, actual owner-host edits, GPU/model installs and npm publication are not authorized.
Unavailable live checks remain not_run.

## Request updates

2026-10-28: owner requested image/video/audio/3D input/output transformations with explicit per-model
output types and modes while retaining image behavior/config, plus complete active branding as
Numera Gen MCP and repository `Numera-Gen-MCP`. The additive source targets 0.3.0; final hosted
qualification remains separate from historical 0.2.0. Private data and immutable signed assets stay
unchanged. Maintained Rust tooling migration retains the five PowerShell scripts.

2026-10-02: owner corrected local root (spaced name), required moving all work/preserving assets and
removing empty wrong root. Completed relocation.
2026-10-02: owner requested GitHub About and topics; descriptive metadata and12 matching topics verified.
2026-10-03: owner requested simple one-file endpoint/key/multiple model IDs with display names,
merging completed side branches/larger logo, Linux/macOS usability and new platform-specific CI tests.
Inline key is private-only; defaults replace mandatory wizard/path/env setup, not privacy/protocol checks.
