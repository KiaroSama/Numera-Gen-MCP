# Testing

## Native Rust migration — pending hosted verification

Hosted revision `05ed820` passed selected native runtime suites and actual two-client HTTP SDK
acceptance on completed targets; strict formatting still blocked publication. Its measured coverage
was 78.33% executable regions / 70.93% branches. The following `b4e466a` runtime fixtures passed,
including active SSE shutdown; coverage reached 81.87% regions / 76.67% branches, still below the
unchanged 85% / 80% gates. Formatting and one nested-condition Clippy warning remained. Packaged distribution and full baseline-case migration acceptance have not passed. Existing Node evidence below is historical regression
evidence, not proof that the Rust migration passes. Hosted stable jobs run
rustfmt, strict Clippy, all discovered Rust suites, native release compilation and archive checks on
Windows/Linux/macOS x86_64/ARM64. A separate isolated nightly job enforces 85% executable regions and
80% branches from actual LLVM summaries, with no runtime exclusion or weakened floor.

The exact Node baseline generates independent config/canonical/image/policy/wire oracle values in CI.
Native tests require nonempty fixtures and compare real decoded original bytes/metadata. Real child
`bridge` tests launch without a launcher from another CWD, list all ten tools and require bounded EOF
shutdown. The hosted interoperability fixture also exercises two actual SDK hosts sharing source-seeded
SQLite receipts and restarts both twice, preserving original bytes and asserting that recovery sends
no generation POST. Transport/auth/poll errors remain distinct from terminal provider states. This
fixture passed on all six targets at `3f27fb9`, using 25 source-seeded jobs, not live providers.
The coverage workflow instruments the same external acceptance path before generating its report;
a passing external fixture alone does not establish the coverage floor. The next candidate adds
source-generated final SSE/result normalization, actual multipart capture, targeted cancellation,
public generation/editing, output recovery and conservative registration fixtures. These additions
passed their selected runtime paths in the next hosted candidate, except a new normalization oracle
exposed an unwanted `usageMetadata` fallback; that production drift is being repaired. Complete parity
and coverage acceptance remain pending. Direct HTTP/menu adds three further Rust targets and an actual
two-client HTTP SDK fixture, now exercised in hosted CI. The next expanded tree contains 38 targets,
including config, expiry, continuation/recovery, native entry-point and Comfy result cases. Those
additions passed selected runtime checks in `b4e466a`; archive verification is still gated by strict
quality checks. The next tree adds bounded admission/Comfy completion, existing-job recovery, image
route/deviation, API preflight and real filesystem/SQL publication-gap fixtures (43 Rust targets).
These newest additions remain pending exact-SHA acceptance. Raw-text JSON oracle checks now cover
JavaScript nonfinite numbers, lone UTF-16 values/keys, duplicate insertion order and binary64 rounding.
All nine tests passed on all six targets at `dc0fb4c`, together with the new UTF-16 access and direct
schema suites. The Windows HTTP cleanup regression also passed after an actual handler-release barrier
was added, without a cleanup retry or longer sleep. All 46 suites, strict Clippy and actual stdio/HTTP
SDK acceptance passed on that revision; formatting still failed. Actual coverage measured 84.43%
regions (below85%) and 80.91% branches (above80%). These are prior-revision observations, not current
integration acceptance. The authoritative value model is now being connected through configuration,
requests, adapter payloads, provider results and persistence. Typed request extensions preserve raw
tool arguments outside SDK serde metadata; boundary codecs restore UTF-16 request IDs and structured
results for both stdio and HTTP. These integration paths and new raw SQLite TEXT compatibility cases
remain pending hosted verification. Coverage reports are retained even after instrumented test
failures; those failures still fail the required gate. The first integrated revision `ed07460` failed
native compilation on two shared type-boundary errors; its tests and coverage are not passing evidence.
Those errors are being repaired together using actual hosted diagnostics and formatter/lock artifacts.
Further acceptance fixtures cover real killed-child checkpoints, queued credential snapshots, two-process
journal budgets, captured edit masks, catalog pagination, malformed Comfy uploads and hanging-response
socket cleanup. Their authored existence is not execution proof or a full 470-case mapping. Revision `4c9e8c7` still
failed native test compilation and strict linting; its partial instrumentation report is not a complete
coverage measurement. Those exact type/lint diagnostics are repaired in one replacement batch.
The Windows source fixture also incorrectly required numeric PIDs never to repeat after reaping;
process-instance checks now use the actual child's immutable startup-log `run_id`, while simultaneous
PID distinction, bounded shutdown and every original lifecycle assertion remain required. Native
stdio negotiation separately preserves the frozen source's legacy/auto behavior and modern-pin
rejection; the optional HTTP transport retains its SDK-supported modern protocol behavior. SQLite
fixture stores, loopback services and owned child processes must close and clean up.
Revision `da6056f` passed all three Node jobs but failed native quality/runtime gates. Its failed
instrumented run measured 83.51% regions / 80.40% branches, not a complete acceptance measurement.
Repairs preserve empty SSE priming events, select the actual response by ID, restore the source's
method-not-found bootstrap fallback, compare canonical Windows paths and construct crash/provider
fixtures through their real admission and allowlist contracts. Revision `ed07a39` passed strict Clippy,
all 54 native targets on POSIX and actual stdio/HTTP SDK acceptance; Windows exposed a published-target
fixture path mismatch. Formatting still failed, and a complete instrumented run measured 84.15%
regions / 80.91% branches. The next tree uses the real canonical publication path and adds actual
HTTP output/preview/resource/error/existing-handle contracts. At `3655fdc`, all 55 native targets, strict Clippy and actual stdio/HTTP SDK acceptance passed on
all six targets, including the Windows crash fixture and public output/resource test. Formatting
still failed in two test files. Complete coverage reached 84.63% regions / 81.10% branches, still
below the region floor. At `8271b81`, the retained original-route, ambiguous-history refusal and
credential-free recovery regression passed. Five stable platforms passed all 55 runtime targets;
Linux x86_64 hit the existing eight-variant asynchronous polling fixture's aggregate eight-second
deadline. All six targets passed strict Clippy and actual SDK stdio/HTTP acceptance. Formatting
failed in one test file. Complete nightly coverage measured 84.72% regions / 81.17% branches: the
region floor still failed. The repair isolates every adapter/state variant as a named harness case
without changing the real polling interval, eight-second per-case ceiling or two-worker cap. New
Comfy fixtures capture distinct reference/mask bytes and workflow targets, zero-upload preflight
failures, GPU refusal/opt-out and completed-original download recovery. At `bc34fbf`, all 56 native
targets, strict Clippy and SDK stdio/HTTP acceptance passed on all six native targets. The polling
fixture now ran 14 cases and passed; Linux x86_64 target duration was 3.02 seconds, versus the prior
8.32-second failed aggregate run. This is an observed run, not a repeated performance benchmark.
Two test files still failed formatting. Complete nightly coverage measured 84.86% regions / 81.42%
branches, still below 85% regions. The Node macOS job separately failed its retained maintenance
runner's cleanup-verification record; original cleanup assertions remain required. The next repair
adds named phase/errno diagnostics and actual close-event identity rather than probing a reusable
numeric PID. Real process-group disappearance remains mandatory within the existing two-second bound.

A pending [source mapping](testing/native-parity.json) records all 50 frozen suite hashes, concrete
native counterparts and unresolved assertion gaps; it explicitly does not mark full case equivalence.
Hosted case collection uses the frozen Vitest runtime to expand declaration loops without running test
bodies. Unlike CLI `list` JSON, it retains skipped platform cases with their names/locations/modes.
Collection must produce exactly 50 suites / 470 cases and close its pool under a 60-second wall /
30-second idle ceiling with two workers. At `3c5958f`, collection produced all 470 cases on Windows,
Linux and macOS with identical case identities and the expected three/one/one platform skips. All
56 native targets, strict Clippy and SDK stdio/HTTP acceptance passed on all six targets; the three
Node jobs also passed, including the cleanup regression. Complete coverage passed unchanged floors
at 85.25% regions / 82.18% branches. One native test-file formatting correction still failed and
prevented archive checks. Raw collection is evidence for mapping, not native assertion equivalence.
Full case-ledger and archive acceptance are still required. At `0870b9e`, formatting, strict Clippy,
all 56 native targets, release builds and SDK stdio/HTTP acceptance passed on five targets. Intel
macOS stopped before those checks when the official toolchain download hit its 30-second socket
read timeout. All three Node jobs and complete 85.25% / 82.18% native coverage passed. Five native
jobs then failed actual dependency-notice preservation; archives and extraction remained unexecuted.
A locked `alloc-stdlib 0.3.0` archive deterministically omits its upstream BSD notice. The repair
pins the exact original notice and provenance, retains all normal notices, and fails unknown missing
notices with sanitized package/code evidence. Official public downloads retain the 30-second socket
idle bound, discard partial timeout attempts, and run under 90-second wall / 35-second idle ownership;
checksums remain mandatory before execution. Offline tooling contracts are run in hosted stable jobs.
Six independent provider HTTP cases are now authored for exact POST/path/auth/Unicode body and original
normalization. At `0f80e3b`, all six provider cases, all 57 native targets, strict Clippy,
release builds, five offline tooling contracts and SDK stdio/HTTP acceptance passed on all six
platforms. Complete coverage remained 85.25% regions / 82.18% branches. One provider fixture
formatting correction still blocked packaging, so the real notice collector and extracted archives
remain unverified. The Linux Node job separately hit its retained PowerShell fixture's ten-second
idle deadline; the other two Node jobs passed. These runtime results are not complete source-case
equivalence. The retained PowerShell fixture now forwards real child output as it arrives and emits
bounded parse/action phase evidence instead of withholding all progress behind `spawnSync`. Its
12-second wall / 10-second idle ceilings and all syntax, WhatIf and no-mutation assertions remain.
The original slow phase is unproven; replacement three-OS execution is required.
The [case ledger](testing/native-cases/index.json) now retains every one of the 470 actual identities,
frozen source hashes and three-platform modes. All remain pending. Targeted cases have literal
source/native assertion ranges and explicit schedule gaps; neither a passing suite nor a completed
boolean can close them. Hosted structural validation compares the ledger with fresh frozen runtime
collection. Full acceptance requires reviewed assertions, no gaps, unchanged native source hashes
and actual applicable native execution evidence. At `3d79621`, formatting and every stable native
quality/runtime/SDK gate passed on all six targets; all three Node jobs and unchanged coverage floors
also passed. The 470-case structural ledger matched fresh collection on all three OSes, while all
470 assertion dispositions remained pending. All eight offline tooling contracts passed on six
targets. Actual notice collection reported only `rmcp 3.5.1` missing its packaged license; its
complete exact-revision upstream notice is now preserved, including the licensing-transition terms.
Archives and extracted execution still wait for successful notice preservation. Seventy configuration
cases now have individually recorded oracle-row/direct-test navigation and exact gaps, rather than
suite-wide coverage claims. Three newly authored directory race unit tests hold two real missing-path
observations before regular-file or link insertion, including an actual Windows junction. They retain
the production metadata, mkdir and containment checks through a private observation seam; no public
test switch, wider path permission or new dependency is added. At `ec60ae3`, all three race cases
passed on four POSIX targets. Both Windows jobs failed only the junction-creation command: the fixture
passed a canonical verbatim path to the shell, but the exact command failure cause remains unproven.
The correction uses its owned non-verbatim parent CWD
and relative junction basename without changing production containment or the two-second child bound.
All three Node jobs passed. Complete coverage passed at 85.37% regions / 82.20% branches. Actual
notice preservation, allowlisted archives, checksums and extracted stdio/HTTP SDK execution passed
on all four POSIX targets; Windows packaging remained gated by that fixture failure. These are hosted
artifacts, not a verified Release or complete 470-case assertion acceptance.

The next tree explicitly registers 59 integration targets. New configuration acceptance cases preserve
exact frozen invalid-input/profile/precedence/compact-default assertions. A pinned-source requirements
oracle covers six edge scenarios and all 56 Flash / 30 Pro documented pixel pairs. Native comparisons
retain exact deviation/warning strings; normal finalization and real SQLite commit-failure recovery
verify original bytes, stable output identity, persisted requirements and repeated public reads.
An opaque-tier recovery case requires exactly one source warning. Recovery deliberately retains
`storage_error` and `partial_or_failed` storage outcome, rather than implying clean storage. These
new cases and the Windows command correction are source-only until replacement hosted verification.
Metadata comparisons and JSON round trips are not image inference or process-restart evidence.
At `3dc882a`, all four requirements tests passed on all six native targets, including exact warnings,
normal publication and SQL-failure local recovery. Strict Clippy, release builds and SDK stdio/HTTP
acceptance passed on all six targets; all three Node jobs passed. The new compact Responses fixture
incorrectly compared the entire schema-defaulted model override instead of the source's requested
settings subset. Windows junction creation still returned exit1; its exact cause remains unproven.
Four files also required hosted formatting corrections. The failed instrumented run measured 85.55%
regions / 82.61% branches, but is not accepted coverage evidence because the config case failed.
Archives were gated. The combined repair applies the actual formatter, compares exact model defaults
and uses fully relative junction source/destination arguments under the owned fixture root. It still
requires replacement hosted proof; no source assertion or timeout is removed.
At `76d32e9`, all ten jobs passed: all 59 native integration targets, real Windows junction schedules,
strict quality checks, release builds, all-six dependency notices/archives/checksums/extracted SDK
execution and all three Node jobs. Complete native coverage passed at 85.51% regions / 82.61% branches.
The case ledger still has 470 pending dispositions; this is not complete assertion parity or a Release.
New source-only acceptance fixtures extend exact policy precedence/default immutability, six nested
reserved keys, scalar/allowlist/array/object/depth limits, URL references, mask polarity/model limits,
compatibility/file-loader failures, no-probe health, one actual503 catalog refresh and private bearer
file variants. A separate frozen-source helper now supplies the exact six Persian prompt-only wire
inputs, both Antigravity reference refusals, all 24 malformed sibling pairs, five fatal envelopes and
two private continuation inputs. Native assertions require three-item order, both exact errors and the
original source-generated 8x6 RGBA image. Actual generation additionally exercises the source's count2,
valid-plus-null response, partial receipt, original bytes, explicit error code and one POST after repeat.
These bring the authored tree to 61 integration targets and require separate hosted execution. Failed
catalog-refresh assertions now close and join their fixture before propagating a panic. Its client
retains the 15-second discovery default; the scenario and fixture socket are bounded at eight and three
seconds respectively. A local helper check against the retained older `dist` failed because that runtime
predates sibling isolation; it is not frozen-baseline or native evidence. The old runtime was not rebuilt
or changed. No extra paid request, live host edit or old-runtime cleanup is part of these fixtures.
At `7f7b800`, all 61 native targets, strict Clippy, release builds and actual SDK stdio/HTTP checks
passed on all six platforms. The new config, policy, normalization and count2 partial-publication
cases passed, as did all three Node jobs. Complete nightly coverage passed at 85.65% regions / 83.02%
branches. Formatting in four test files alone failed the six stable jobs, so notice/archive/extraction
steps were skipped. The replacement applies the exact hosted formatter and rebases assertion ranges;
no assertion or timeout changes. All 470 case dispositions remain pending, with 130 assignments.
At `7199441`, the replacement run passed all ten jobs, all 61 native integration targets, strict
Clippy, release builds, complete coverage at 85.65% regions / 83.02% branches, and actual dependency
notices, allowlisted archives, checksums and extracted SDK execution on all six native targets.
This verifies that revision's distribution checks, not complete baseline assertion acceptance.

The next source-only batch adds one registered environment acceptance target (62 targets total):
actual `.env` file loads for all 17 expanded source cases, exact URL/redaction inputs and exact
source-generated 8x6 PNG/JPEG/WebP originals. It directly asserts four-MiB Base64, byte/pixel limits,
real mask dimensions, bounded preview geometry, Unicode file URLs, escaping input links/output
junctions, tampered/missing owned outputs and a regular-file output root. The root-failure fixture
uses a valid prepared/reserved/retained item, exact storage error and preserved pending original;
an unrelated missing request cannot satisfy it. The 82 reviewed case records bring concrete
assignments to 212, still with all 470 dispositions pending. Native compilation, formatting and
execution of this new batch have not run locally and require separate hosted evidence.
Legacy same-request/same-index fresh-UUID publication and the public `addOutput` contract remain
explicit gaps: distinct stable items and directly seeded SQL metadata do not prove those contracts.
Pure sibling-containment and the complete bounded-stream/cancellation input case also remain open.
At `9e6e234`, all 62 native targets, strict Clippy, release builds and SDK checks passed on five
completed native platforms, as did nightly tests and complete 85.80% regions / 83.55% branches.
The macOS ARM native and Node jobs were cancelled, not passing. Six Rust fixture files and the
JavaScript oracle import needed formatting; the two completed Node jobs stopped at that format
check, and all distribution packaging was skipped. The repair uses the actual hosted formatter
without changing assertions. Exact-source review also found blocked readiness returning exit1
instead of exit2; a scoped correction and four actual-executable readiness fixtures are authored
with exact catalog/error/credential inputs, report fields, GET-only/zero-call and redaction assertions.
Those new fixtures are not yet executed. Native readiness runs directly, so they verify the actual
child's reaping rather than inventing the old inner-stdio wrapper's cleanup log marker. That transport
mapping gap remains explicit. Four previously substituted request fixtures now use the exact `draw`,
`../id`, empty reference-path and `extra` inputs. The integrated repair has 63 registered native
targets and 216 concrete case assignments, with all 470 dispositions still pending. Independent
source review found no additional confirmed defects in the readiness repair; it is not runtime proof.
Replacement hosted verification is still required.
At `69e46ee`, all 63 native targets passed on five platforms and nightly; Windows ARM passed three
readiness cases but failed the fourth fixture's directory cleanup with `Uncategorized`, without a
raw OS-code diagnostic. All platforms passed Clippy, release builds and SDK checks. Full coverage
passed at 85.85% regions / 83.82% branches. Linux and Windows Node jobs passed; macOS failed one
POSIX cleanup-record assertion after a post-close group kill returned EPERM and the final probe
established group absence. Native readiness formatting also failed, and packaging was skipped.
The repair keeps unsuccessful signal attempts as diagnostics but resolves only the exact EPERM
attempt after child closure and definitive group absence; permission/probe/deadline/survivor failures
remain failures. Seven deterministic classification fixtures preserve this distinction. Windows
fixture cleanup now reports the raw OS code and retries only documented sharing/delete-pending/
not-empty errors during a two-second retry window. That window limits additional attempts, not the
duration of a synchronous OS deletion call; the original failure's precise cause remains unproven.
The independent-publication source gap is now implemented: two saves with the same request/index
and no receipt create fresh UUID paths, and `Store.add_output` preserves the unchanged SQL schema.
Stable production callers explicitly retain the journal, writer fence and recovery path. Exact
original/tampered/missing/outside-owned assertions are authored and independently source-reviewed,
not yet executed for this new tree. All 470 dispositions remain pending; no source case is promoted
from these source-only repairs, coverage or an older successful run.
At `98cc779`, all 63 native targets, Clippy, release builds and SDK checks passed on all six native
platforms; all three Node jobs passed, including the macOS cleanup regression. Complete native
coverage passed at 85.85% regions / 83.82% branches. Only formatting in `native/main.rs` and the
shared fixture helper failed; archives remained skipped. The Windows cleanup failure did not recur,
but that does not identify its earlier unknown OS cause. Exact hosted formatting is now applied.
The next source-only batch registers 65 native targets and records 271 case assignments. It adds
literal source stream normalization/JSON/SSE/binary cases, exact seven-byte preview/completion
schedules, actual two-reference multipart capture with alpha mask, sibling containment and the exact
three-byte bounded-body fixture. The shared API/asset accumulator rejects missing bodies separately
from empty present bodies. The 9router builder now preserves explicit multi-reference JSON and
rejects unverified implicit non-Codex edits, matching the frozen source rather than inferring current
account entitlement. These new assertions and repairs require their own hosted execution; all 470
case dispositions remain pending. Windows/Linux/macOS x64 and ARM64 tests and builds remain exclusively
on GitHub-hosted runners; no local Rust build or toolchain is used.

At `d9fc7be`, all 65 native targets passed on all six platforms and nightly; Clippy, release builds,
stdio/HTTP SDK checks and all three Node jobs also passed. Complete coverage passed at 86.08% regions /
84.32% branches. Only formatting in five test files failed, so notices, archives and extracted-package
checks were skipped. The actual hosted formatter corrections preserve every assertion and string.
The next acceptance repair adds exact direct-finalization fixtures: source 8x6 originals, ordered
JPEG deviations, aggregate limits, cancellation and recovery on the same Generation/Store, data-URL
output and terminal cancellation. Direct finalization now claims, renews and releases its writer;
already-claimed callers preserve their outer ownership. A controlled failure checks release on error.
Terminal retention updates receipt JSON without relinquishing the writer's full reservation. An exact
1024-byte budget/32-byte result case refuses a peer's 992-byte reservation before release and permits
it afterward. These new changes are source-reviewed but not yet compiled or executed. The withdrawn
hypothesis that same-owner recovery necessarily blocks is not a repair rationale or test result.
The 46-case Store/lifecycle review retains partial and unmapped schedules explicitly. The current
batch has 66 registered native targets and 313 concrete assertion assignments; many remain partial.
All 470 acceptance dispositions remain pending; passing related fixtures does not establish literal parity.

At `39c9643`, all 66 native targets, Clippy, release builds and stdio/HTTP SDK checks passed on all six
platforms; all three Node jobs passed. Complete coverage passed at 86.13% regions / 84.49% branches.
The direct-finalization and intermediate reservation regressions passed. Two formatting corrections
in the new finalization fixture were the only failures, leaving notices and archives skipped.
The next source-only target adds the seven literal Discovery-model dimension variants and two minimal
endpoint-descriptor cases, without adding provider fields absent from the source fixtures. It uses
bounded loopback only for descriptor reads and keeps native capability, forwarding and account
availability separate. The current batch has 67 registered native targets and 322 concrete assertion
assignments, including partial mappings; all 470 dispositions remain pending. Its compilation and
execution remain pending.

CI compiler/dependency caches, target/build output and TEMP/TMP/TMPDIR stay inside the workspace, with
two workers and documented wall/idle ceilings. No Rust toolchain install/build/full test runs locally
for this migration. [Acceptance matrix](testing/MIGRATION-MATRIX.md) records incomplete source-case,
crash/restart, multipart, packaging and live/GUI verification rather than calling them passing.

## Historical Node implementation

The native TypeScript7.0.2 compiler runs build/typecheck single-threaded through an explicit
@typescript/native npm alias. Type-aware ESLint retains the supported TypeScript6.0.3 API via
Microsoft's @typescript/typescript6 compatibility alias. Normal npm ci resolves both without forced
peers or parser patches. The check entrypoint asserts both versions and the lint API before running
existing strict checks; the three-platform matrix also validates the native compiler's emitted runtime.
See [Microsoft's side-by-side guidance](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0).

npm run check: strict typecheck/lint/build/UTF8/source-size. npm run coverage: all discovered unit,
contract and integration files with85 percent statements/80 percent branches. No runtime exclusions.
Outer runner180s wall/45s idle, case10s/hooks15s, max2 workers. POSIX cleanup waits at most2s
for the killed process group to disappear; it never treats a live group as verified cleanup. Tiny cached Sharp fixtures and isolated
project-owned temp roots; loopback external boundaries only, finally cleanup stores/loggers/sockets.

npm run package:check validates allowlisted npm artifact and a production-only clean install in a
path with spaces using a real MCP client. npm run smoke performs provider-free handshake/tools/health.
New tests/integration/posix-platform.test.ts runs on Linux/macOS: owner-only0600 secrets vs0644,
Unicode native config paths, symlink escape rejection and actual bounded POSIX process-group cleanup.
Windows skips those three OS-specific assertions; DPAPI remains its separate Windows-only assertion.
Common compact/stdio/packed tests run on every OS. Hosted CI Windows/Linux/macOS on Node24.21.0; Windows hosted is not Windows11 Enterprise/Server2025 proof.
Runtime dependencies audited. All test files matched automatically by Vitest include; no hand-list gap.

Local light checks ran only to drive implementation. Full final matrix is reserved for CI. Generic
SDK fixture interoperability is not a Claude/Codex GUI test. Synthetic images verify bytes/container/
metadata/path semantics, not real model quality or editing fidelity. No paid default tests.
Setup real-entry fixtures verify locked concurrent JSON/TOML registration, backup restoration,
conservative TOML refusal, five PowerShell7 WhatIf actions and idle-timeout descendant cleanup.
Logging fixtures verify immutable per-run metadata, closed file handlers, redaction at all levels;
actual stdio tests assert paired tool start/completion/failure with call IDs/durations and no key/prompt.
Native wire-contract fixtures assert method/path/header/body for each protocol, multipart byte fidelity,
legacy-client negotiation, endpoint descriptors and private continuation state. Recovery regressions
exercise renewable writer fencing, expired prepared admission, retained inline bytes after cancellation,
local recovery without connection credentials, published-file/hash reconciliation, bounded journal
reservations, real output-index SQL-trigger failure, future-schema refusal, and legacy migration.
Actual child kill checkpoints cover pre-intent/prepared/submitted/running/finalizing/retained/published/
committed states; reopened stores assert deduplication and original hashes. Original Comfy waiting
races a refresher through the actual Generation entry point; partial downloads retain the first image.
These cases passed fresh hosted CI on320f903; historical CI is not used as their evidence.
Scheduled filesystem races insert a directory, regular file or
junction between the missing-path observation and mkdir; only the real directory is accepted.
Two distinct reference images prove scalar contracts reject and array encoding preserves both.
Dimension fixtures separate sourced model limits from the route safety ceiling and compare persisted
aspect/tier requirements against original output bytes.
No controlled real-gateway process was run against fake upstreams; gateway-shaped contract fixtures
must not be called deployed-gateway integration. Interactive DPAPI creation/ACL remains not_run.

Live tests require endpoint/model/number/cost authorization. No existing key authorizes generation.
GPU actual graph/model/backend and host GUI tests not_run when unavailable. Keep evidence separate
from configured/documented/advertised support. See TEST-RESULTS.md and REQUIREMENT-TEST-MATRIX.md.

Terminal lifecycle fixtures exercise public MCP failed/cancelled results, polling error/identity controls,
targeted cancellation dispatch versus confirmation and database reopen. Reservation fixtures preserve
unknown receipts, retained bytes/URLs and live/detached budgets; actual child processes race one bounded
journal budget. Local recovery preserves terminal generation state and original bytes. These are
synthetic provider contracts, not live Comfy/Gemini/OpenAI cancellation or refund proof.

Real stdio terminal acceptance: stdio-terminal-lifecycle.test.ts runs two actual built MCP processes
sharing isolated SQLite, restarts both twice, and verifies Responses/Interactions terminal variants,
polling controls and targeted Comfy cancellation through public tools. Public generation probes verify
released versus retained admission without replaying old jobs. stdio-terminal-races.test.ts gates actual
waiting/refresher/cancellation races and completion-before-download precedence; original bytes and output
identities survive both restarts. The host fixture drains bounded stderr, limits calls/startup to5s and
child lifetime to45s, closes and observes real process exit. Cases are bounded at30s/60s; max2 CI workers
and existing coverage gates remain. These tests use loopback synthetic providers, not live accounts. A peer refresher must respect a live
writer in the other process rather than steal its lease. Explicit job recovery ignores prior local wait
cancellation for original-byte publication. Concurrent first-open WAL contention is bounded separately
from provider calls; no network submission is retried.
