# Architecture

Native Rust CLI `bridge` -> bounded stdio RMCP transport -> schema-validated ten tools -> application
services -> pure adapter mapping -> bounded HTTP -> normalization -> validated original outputs and
transactional receipts. Native implementation and distribution verification are in progress; the
migration matrix records acceptance rather than implying that authored modules have passed tests.

## Boundaries

- `native/config/`: full/compact/dotenv config, defaults/precedence, private credentials/current-user
  Windows DPAPI and immutable last-good reload snapshots. Adjacent discovery is executable-relative,
  never caller CWD. Display names never replace exact model IDs. API credentials are pinned privately
  for the operation and not serialized into receipts/logs.
- `native/adapters/`: Images, Gemini, Interactions, Responses, dedicated OpenRouter, opt-in chat images
  and explicit ComfyUI workflow contracts. Tools construct no provider payloads. Gateway forwarding,
  native model capability and account entitlement are separate evidence layers.
- `native/services/`: discovery, readiness, generation, admission, finalization and existing-handle
  recovery. Intent is persisted before paid submission; unknown outcomes are never resubmitted.
  Cancellation of local waiting does not establish upstream cancellation or a refund.
- `native/jobs/`: bundled SQLite, preserved schema v2/WAL/FULL, renewable 30-second leases and
  generation fences. Journal phases waiting/retained/publishing/committed keep stable output identity
  across filesystem/SQL crash gaps. State remains local shared storage, not an unreliable network drive.
- `native/files/`, `native/http/`, `native/mcp/`, `native/cli/`: byte/container/path/mask validation,
  API transport separate from DNS-pinned credential-free assets, SDK protocol lifecycle and optional
  explicit host registration. Originals are never resized; derivative previews are separate.

`native/runtime_paths.rs` confines managed temp/cache to the installation root. Logging uses private
UTF-8 UTC structured files and stderr, never protocol stdout; credentials/prompts/private paths stay
redacted at every level. The executable needs no Node/Python/Rust installation. ComfyUI, if selected,
owns inference and GPU/model/node installation; Numera does not bundle or silently install them.

## Decisions and recovery

Same request ID plus identical effective inputs/account returns the same durable receipt; a changed
identity conflicts. Canonical hashing preserves source JavaScript number/key/English-collation behavior
for existing records; actual oracle evidence is required before accepting compatibility. Existing
handles use retained matching connection snapshots; ambiguous destinations refuse network recovery.

Completed inline bytes are retained before observing cancellation or publishing files. Existing file
publication is reconciled by output UUID/target/hash, never overwritten. Local completed bytes can be
recovered even when a connection has been removed. Invalid saves retain last-good config; changes to
storage/security/global policy or the 64-snapshot bound require restart.

Hosted CI resolves the real Cargo lockfile, verifies stable native binaries per OS/CPU and packages
allowlisted archives. A separate nightly job measures executable regions >=85% and branches >=80%;
nightly is not the production compiler. The pinned Node source is used only in hosted migration oracle
checks, not as a runtime dependency of the shipped application.

Keep old local runtime/dependencies until the owner switches host config and the native connection is
verified. Migration snapshots, original images, receipts, private configuration and Git history survive
cleanup. [Migration acceptance](testing/MIGRATION-MATRIX.md) lists gaps; source existence is not evidence
that any runtime or coverage gate passed.
