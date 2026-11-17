# Media recovery and credential regression contracts

The queue acknowledgement is not permission to submit again. Recovery reads only the original
handle or retained originals. None of these repairs adds paid POST retries, provider fallback,
transcoding, public uploads, credential persistence, or a relaxed destination policy.

## Operation credentials

`generate_media(wait=true)` carries the operation-only `PinnedConnection` from submission through
all status polls and result retrieval, under the original deadline. Rotating a secret file between
those steps cannot switch the account for an already accepted operation. A new independent request
takes a fresh snapshot. A later explicit refresh also takes a fresh snapshot and must still match
the persisted destination/account identity before any request is sent. Local original recovery does
not require provider credentials.

## Durable original recovery

Active finalization errors carry `artifact_index`, and errors for retained originals also carry their
durable `output_id`. Identical repeated errors are deduplicated. A successful publication moves only
that original's errors to `recovered_errors`; unresolved sibling errors remain active and keep the
receipt partial. History is not interpreted as a current failure.

The original's output metadata and committed journal phase are published atomically by the existing
store. On a resumed finalization, an error linked to an already committed output UUID is reconciled
before new work. This handles interruption between original publication and the last receipt update
without another download or generation.

Older receipts lack artifact indexes. Their unindexed `input_file_error` with stage `download` is
retired only when no retained original is pending and at least one original has been published.
Malformed artifact and other unrelated errors are not removed by that compatibility rule. A failed
retry never counts as recovery.

## Hunyuan3D variants

The reviewed `fal-ai/hunyuan3d-v3/text-to-3d` schema includes `model_urls.usdz`, alongside GLB,
OBJ and FBX. Numera recognizes the known USDZ sidecar but, as with OBJ/FBX, does not download or
advertise validation of it. The primary verified GLB remains usable. Unknown non-null variant roles
remain rejected, duplicate primary GLB URLs are deduplicated, and the shared 64-artifact bound remains.

## Persistent regressions

`tests/rust/media_acceptance/audit.rs` uses the real MCP SDK, real server processes, loopback provider
HTTP, private temporary state, and original media bytes. Its cases cover a key rotation immediately
after POST and between consecutive polls; new-operation credential refresh; valid GLB with a USDZ
sidecar; download failure followed by process restart and successful recovery; recovery with an
unresolved malformed sibling; byte/hash/identity stability; one paid submission; repeated retrieval;
and unknown-role/artifact-count rejection.

The finalizer unit contracts additionally exercise deduplication, stable history, publication-before-
receipt reconciliation, and preservation of an unpublished validation failure.

These are deterministic protocol and storage tests, not live provider account, billing, real GUI,
GPU inference, or upstream cancellation/refund evidence. Native runtime validation is hosted CI only.
Run the existing `CI` workflow with `operation=qualify` on the exact candidate commit and inspect every
native execution, coverage and evidence-aggregation gate. Do not infer a passing qualification from
formatting, source inspection, historical evidence, skipped work, or a successful individual harness.
