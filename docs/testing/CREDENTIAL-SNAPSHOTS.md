# Native credential snapshot acceptance

## Current evidence

At `3bc2789`, all ten jobs passed: all 94 native targets and both credential cases/module regressions
on six stable OS/CPU targets and nightly. All-six fmt/Clippy/release/SDK stdio/HTTP/notices/licenses/
archives/checksums/extracted SDK and three Node jobs passed. Complete coverage was 86.86% regions /
85.73% branches. The prior admission timeout's cause remains unknown despite its unchanged probe passing.
At `b23f6e2`, Windows x64 passed full DPAPI/ACL/native Unicode round-trip/corrupt/no-overwrite checks;
ARM reached successful protection but failed before ACL verification. Both descendant-abort cases
passed. Forwarding the omitted `PATHEXT` corrected a reproduced lookup defect, not every ARM cause.
All-six Clippy/release/SDK and nightly SDK passed; coverage was 86.90% / 85.63%. Formatting kept CI red
and archives were skipped. Exact formatting and private ACL-stage/type diagnostics await hosted CI.
The diagnostic catch checks at most eight nested exceptions and emits fixed booleans only; original
ACL expressions and failure assertions remain. No raw capture, path, plaintext or ciphertext is reported.
Parent-owned noninheritable kill-on-close jobs contain a self-enrolled worker before helper launch;
cleanup reaps that worker before terminating its job and requiring zero active processes under one
two-second ceiling. The original 30-second case and 15-second wall/8-second idle source wrapper remain.
Node and PowerShell are CI fixtures only, not native runtime dependencies. The current Comfy-polling
addition brings 96 targets/467 assignments/three unassigned/all 470 pending. No acceptance promotion.

## Selected-key environment source

`native/config/runtime_environment.rs` implements `EnvironmentSource` for immutable startup maps
and an instance-owned `Environment` backed by `Arc<RwLock<HashMap<String, String>>>`. Credential
resolution reads only the selected `secretEnv` value and returns an owned string before any await.
Inline, private-file, DPAPI and unauthenticated branches keep their existing precedence and do not
read the shared map unnecessarily. Invalid assignments and poisoned locks return fixed sanitized
errors, never credential values or lock contents. Header validation remains in the credential resolver.

Application construction converts its startup map once. Accepted configuration generations clone
the shared handle; config loading and reload parsing retain their immutable startup maps. Generation
pins private headers before admission waiting. Existing-job recovery supplied with a pinned connection
uses that connection, and cancellation passes the same pinned connection into nested completed-job
recovery. Discovery pagination pins once; configured/cached discovery and non-probing health do not
add an environment dependency. Receipts, configuration and logs do not serialize the environment.

## Frozen-source cases

`tests/rust/environment_snapshot_acceptance.rs` maps only these two frozen
`tests/integration/credential-snapshot.test.ts` cases:

- Lines 24–119, environment expansion: one Generation queues with credential A, rotates its selected
  key to B, completes the queued request with A, then starts a fresh operation with B. Assertions
  retain the exact image POST/body, two requests, account identities, original 8x6 alpha PNG bytes
  and no credential values in serialized receipts/configuration.
- Lines 137–226: Comfy cancellation observes a completed status while rotating the same Generation's
  key. Cancel/status/history/view requests all retain A; nested recovery saves the original once,
  performs no generation submission and claims no refund. A fresh credential lookup selects B.

Two module unit regressions separately cover owned selected values, shared clones, independent empty
instances, real deletion, case-sensitive keys, assignment validation and sanitized writer poisoning.
Existing Generation fixtures receive only type/set/remove adaptations; their assertions remain intact.

## Bounds and limitations

The cases use isolated workspaces, tiny existing source-generated originals and loopback fixtures,
not live accounts or paid providers. Each case has a ten-second envelope, an eight-second wire action
and three-second socket bounds. Queue observation is limited to two seconds with a ten-millisecond
state interval. Source provider defaults remain unchanged. Owned current-thread runtimes drop before
workspace cleanup, including panic/timeout paths. The existing hosted two-worker ceiling remains.

The shared map is **not OS `process.env`**: Windows case aliases, JavaScript assignment coercion,
OS propagation, parent-process changes and external mutation of a running process are not verified.
Independent application instances do not share a global environment. Explicit maps remain exact-key;
global case normalization would incorrectly change POSIX and supplied-map behavior.

The two cases call Generation/jobs directly. Application owner selection can resolve credentials
before the later operation pins them again. Rotation between those reads can fail closed with
`permission_denied`; the identity recheck prevents a mismatched-account request. These cases do not
prove one snapshot across the complete public dispatch. Proxy environment lookup is unchanged.

Applicable hosted execution, exact native/dependency/helper hashes and final independent literal
review remain acceptance gates. A passing source review, target registration or earlier green SHA
cannot satisfy them. See [migration matrix](MIGRATION-MATRIX.md) and the
[pending case ledger](native-cases/index.json).
