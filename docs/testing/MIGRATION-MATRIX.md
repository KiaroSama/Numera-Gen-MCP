# Native migration acceptance matrix

Baseline: `fc7da25c3b470199949b1b4c67127d77afa0c34a`, 50 discovered suites / 470 cases.
The Rust implementation is in progress. The integrated `3655fdc` run passed all 55 native targets,
strict Clippy and actual SDK stdio/HTTP acceptance on all six OS/CPU targets, including authoritative
UTF-16/number application values, the repaired Windows crash fixture and public HTTP outputs/resources.
At `8271b81`, the retained-route/ambiguity/local-recovery regression also passed. Five stable platforms
passed all 55 runtime targets; Linux x86_64 exceeded the aggregate asynchronous polling fixture's
eight-second deadline. All six passed strict Clippy and actual SDK stdio/HTTP acceptance. Formatting
failed in one test file. Complete instrumented coverage measured 84.72% regions / 81.17% branches:
the region floor still failed against unchanged 85% / 80% gates. At `bc34fbf`, all 56 native runtime
targets, strict Clippy and SDK stdio/HTTP acceptance passed on all six targets. Complete coverage was
84.86% regions / 81.42% branches; formatting in two test files and the region floor still failed.
The Node macOS maintenance cleanup-verification record also failed. At `3c5958f`, all 56 native targets,
strict Clippy and SDK stdio/HTTP acceptance passed on all six targets, all three Node jobs passed,
and coverage passed at 85.25% regions / 82.18% branches. Dynamic collection produced the same 470
case identities on all three OSes, including platform skips. One native formatting correction still
failed; archive checks did not run. At `0870b9e`, five native targets passed formatting, strict Clippy,
all 56 runtime targets, release builds and SDK stdio/HTTP acceptance. Intel macOS stopped on a
30-second toolchain-download timeout before native checks. All three Node jobs and complete native
coverage passed at 85.25% regions / 82.18% branches. Five actual-notice preservation steps failed;
archive/extraction checks remained skipped. Exact upstream-notice and bounded public-download repairs,
plus six actual provider-HTTP fixtures, were published at `0f80e3b`. All six provider cases, all
57 native targets, strict Clippy, release builds, five offline tooling contracts and SDK acceptance
passed on all six targets; complete coverage remained 85.25% regions / 82.18% branches. One test
formatting correction blocked packaging. Linux's Node PowerShell fixture hit its ten-second idle
bound; the other Node jobs passed. The actual notice collector and archives remain unverified.
At `3d79621`, formatting, all 57 native targets, strict Clippy, release builds and SDK acceptance
passed on all six targets. All three Node jobs and unchanged coverage floors passed. Fresh 470-case
structural validation passed on three OSes without accepting assertion parity. Actual notice
collection identified only `rmcp 3.5.1` missing its packaged license; its complete pinned upstream
notice was subsequently preserved. At `ec60ae3`, actual notices, allowlisted archives, checksums and
extracted stdio/HTTP SDK execution passed on all four POSIX targets. All three Node jobs and full
85.37% regions / 82.20% branches coverage passed. Both Windows jobs failed only the new junction
fixture's shell command; their archive/extraction steps did not run. The fixture correction uses an
owned normal CWD and relative basename, retaining real junction/containment assertions and time bounds.
New config acceptance and source-exact requirement warning/deviation/finalization/recovery cases bring
the authored tree to 59 integration targets. At `3dc882a`, all four requirements cases passed on all
six targets; all three Node jobs and native Clippy/release/SDK checks passed. Packaging remained gated
by formatting, a too-strict compact model-override fixture and Windows junction command exit1. Its
85.55% regions / 82.61% branches report came from a failed instrumented run, not accepted coverage.
The combined repair retains the source settings subset and real junction assertions; replacement
hosted execution remains required. That replacement, `76d32e9`, passed all ten jobs, including all
59 native targets and all-six notices/archives/checksums/extracted SDK execution. Complete coverage
passed at 85.51% regions / 82.61% branches. Windows junction and compact settings cases passed; full
470-case assertion equivalence and Release/cutover remain open. The next source-only policy/config
and exact wire/malformed-result/continuation fixtures bring the authored tree to 61 integration targets;
they are not covered by that green SHA. A new actual count2 partial-publication fixture retains original
bytes and proves one POST after same-instance repeat. All new literal mappings remain pending hosted
execution and final review; an older local runtime cannot supply the frozen normalization expectations.
At `7f7b800`, all 61 native targets, strict Clippy, release builds and SDK stdio/HTTP checks passed on
all six platforms; all three Node jobs and full 85.65% regions / 83.02% branches coverage passed.
Only formatting in four test files failed. Actual notices/archives/extracted execution were skipped
on that revision. Exact hosted formatting and assertion-range corrections still need replacement CI;
130 assignments and all 470 pending dispositions do not establish full source-case acceptance.
At `7199441`, all ten jobs passed, including all 61 native targets, strict quality, complete 85.65%
regions / 83.02% branches and all-six actual notices, archives, checksums and extracted SDK execution.
The new source-only environment/foundation/image batch has 62 registered targets and 212 concrete
case assignments, all still pending native execution and final literal acceptance. Exact file loads,
original codecs, Base64, masks/previews, Unicode paths, actual input/output links and owned-output
failures are now authored. Legacy fresh-UUID same-index publication/public addOutput, pure containment
and the exact bounded-stream/cancellation case remain gaps, not silently retired contracts.
At `9e6e234`, five completed platforms passed all 62 native targets, strict Clippy, release and SDK
checks; complete nightly coverage passed at 85.80% regions / 83.55% branches. Both macOS ARM jobs
were cancelled. Only six Rust-file layouts and the JavaScript import formatting failed completed
jobs; packaging was skipped. Actual hosted formatting is applied without weaker assertions.
A source-confirmed readiness exit1/exit2 drift is corrected with four source-faithful executable
fixtures and exact invalid-request literals. This repair is source-only pending its own hosted run;
direct native maintenance does not establish the old inner-stdio/cleanup-marker assertion.
The integrated repair registers 63 native targets and records 216 case assignments, all 470
pending. Independent source review found no additional confirmed readiness defects, not proof
of compilation or execution.
At `69e46ee`, five platforms and nightly passed all 63 native targets; Windows ARM failed only one
readiness fixture's final directory cleanup. All six passed Clippy, release and SDK checks, but
readiness formatting failed and archives were skipped. Full coverage passed at 85.85% regions /
83.82% branches; macOS Node failed the final POSIX cleanup-state report after a denied kill attempt.
The next unified repair applies hosted formatting, keeps denied attempts separate from definitive
final absence proof, and makes Windows transient fixture cleanup bounded and diagnostic. It also
implements source-compatible fresh-UUID same-index independent saves and `Store.add_output` while
stable callers retain their journal/fences. These changes remain unexecuted natively; pure containment,
exact bounded-stream/cancellation and the old inner-stdio/cleanup-marker acceptance remain open.
At `98cc779`, all six native platforms passed all 63 targets, Clippy, release builds and SDK checks;
all three Node jobs and full 85.85% regions / 83.82% branches coverage passed. Only main/shared-helper
formatting failed; archives were skipped. Exact hosted formatting is applied without changing behavior.
The next 65-target source batch brings concrete assignments to 271, all 470 still pending. Literal
stream/result/builder fixtures, exact incremental schedules, two-reference multipart with mask,
sibling containment and bounded-body/cancellation cases are authored. Source-confirmed 9router
multi-reference and unverified implicit-route divergences are corrected; missing API/asset bodies
are distinct from empty present bodies. Current compilation/execution and final literal acceptance
remain pending. Builds and tests continue on all six GitHub-hosted Windows/Linux/macOS x64/ARM64 targets.

At `d9fc7be`, all 65 native targets passed on all six stable platforms and nightly; all-six Clippy,
release builds and stdio/HTTP SDK checks, all three Node jobs and full 86.08% regions / 84.32% branches
coverage passed. Only five test-file formatting checks failed; notices and all archives remained
skipped. Actual hosted formatting is now applied without assertion/string changes.
The next source-only repair registers a direct-finalization acceptance target with four literal
8x6/default-receipt cases and a direct-error versus claimed-writer control. Direct calls claim/renew/
release; outer callers explicitly retain claimed ownership. Terminal retention preserves the full
reservation until release, with the 1024/32/992 intermediate peer-budget assertion missing from older
fixtures. Independent source review found no confirmed defects, not compilation/runtime proof.
The current batch has 66 registered native targets and 313 concrete assertion assignments, including
partial mappings. All 46 reviewed Store/lifecycle cases retain pending status; remaining public
SDK/reopen/process schedules are not closed by direct Store assertions. The earlier same-owner recovery-blocked
hypothesis was retracted and is not treated as a failure.

At `39c9643`, all 66 native targets and all-six Clippy/release/stdio/HTTP SDK checks passed; Node jobs
passed on all three OSes. Complete coverage passed at 86.13% regions / 84.49% branches. Both lifecycle
repairs passed their new regressions. Only two formatting corrections in one test file failed;
notices and archives were skipped. The next unexecuted target adds seven literal Discovery-model
dimension variants and two source-minimal endpoint-descriptor cases. Independent source review found
no confirmed defects; that is not native compilation or complete case acceptance. The current
67-target batch records 322 concrete assertion assignments, including partial mappings; all 470
acceptance dispositions remain pending.

**All-six packaged execution passed at `7199441`; complete baseline-case assertion acceptance has not passed.** Passing
selected JSON integration fixtures is not full accepted-domain or 470-case parity. Old TypeScript
production source remains authoritative until hosted differential verification passes. A file
existing is not a verified mapping. Polling-case isolation and Comfy upload/preflight/recovery
regressions passed at `bc34fbf`; new reference-encoding assertions and cleanup/case-collector repairs
passed at `3c5958f`. The [pending ledger](native-parity.json) preserves all
50 frozen suite hashes and concrete source-authored mappings, with incomplete case equivalence.
Actual expanded collection retains platform-skipped cases; no suite grouping is promoted to 470-case acceptance.
The [case ledger](native-cases/index.json) has all 470 identities and remains entirely pending.
Targeted cases have concrete assertion ranges; explicit gaps include same-instance recovery versus
reopen, unmasked versus mask upload counts, and the Omni multipart model field. Structural validation
and full assertion acceptance are separate gates; setting a completion flag alone cannot pass.

| Boundary                                              | Native implementation                                                 | Authored verification                                             | Status                                                                                                                    |
| ----------------------------------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Config defaults/full/compact/dotenv/UTF-16/precedence | `native/config/`                                                      | `tests/rust/config_parity.rs`, pinned-source oracle               | Native source-oracle and raw JSON fixtures passed at 8271b81; full case assignment pending                                |
| Credentials/env/private files/current-user DPAPI      | `native/config/credentials.rs`, `dpapi.rs`                            | Config fixture tests; same-user DPAPI oracle generated on Windows | Pending CI; no real credentials transferred                                                                               |
| Hot reload/64 snapshots/last good config              | `native/config/reload.rs`, `native/mcp/application.rs`                | Config reload cases                                               | Retained-route/ambiguity/local recovery passed at 8271b81; exact concurrent/source schedules pending                      |
| Stable request/account identity                       | `native/compat/`, `native/services/identity.rs`                       | Canonical source oracle                                           | Pending ICU/JavaScript identity parity                                                                                    |
| Durable SQLite v2/WAL/FULL/migration                  | `native/jobs/`                                                        | State/lifecycle/recovery cases                                    | Migration and eight real killed-child checkpoints passed at 8271b81; complete fault/reopen matrix pending                 |
| Admission/fences/terminal/reservations                | `native/jobs/lifecycle.rs`, `journal.rs`                              | Lifecycle/recovery cases                                          | Pending multi-host and process-crash coverage                                                                             |
| HTTP/redirect/DNS/assets/SSE/no retries               | `native/http/`                                                        | HTTP cases and generation loopback                                | Pending all bounded network/resource cases                                                                                |
| PNG/JPEG/WebP/original bytes/masks/previews           | `native/files/`                                                       | Real baseline-generated image metadata                            | Pending malformed/animated/container/path race parity                                                                     |
| Images/Gemini/Interactions/Responses/chat/OpenRouter  | `native/adapters/`                                                    | Source-generated wire and normalization cases                     | Pending native multipart capture and terminal SSE cases                                                                   |
| Comfy graph/references/node descriptors/poll/cancel   | `native/adapters/workflow.rs`, `comfy.rs`                             | Source coverage still being mapped                                | Pending complete native contracts                                                                                         |
| Dimensions/capability evidence/model policy           | `native/capabilities.rs`, `dimensions.rs`, `services/model_policy.rs` | Policy source oracle                                              | Pending live-free descriptor contracts                                                                                    |
| Generation/dedup/finalization/local recovery          | `native/services/`                                                    | Tiny loopback, original hash, recovery fixtures                   | Pending exhaustive original lifecycle contracts                                                                           |
| Ten tools/stdio/resources/negotiation/bridge          | `native/mcp/`, `native/main.rs`                                       | Real child `bridge` fixture, foreign CWD and EOF                  | Bridge/two-host/two-restart/legacy-auto-pin and HTTP outputs/resources passed at 8271b81; active race case ledger pending |
| Maintenance/readiness/client registration             | Native CLI replacement                                                | Existing maintenance cases not yet migrated                       | Pending; do not retire working scripts yet                                                                                |
| Native archives/hash/extraction/licenses/docs         | `scripts/native-package.py`                                           | Actual allowlist/checksum/notice/extracted SDK checks             | Passed all six native targets at7199441; no verified Release yet                                                          |
| Coverage                                              | Hosted nightly only                                                   | LLVM regions >=85%, branches >=80%                                | Complete7199441 report:85.65% regions /83.02% branches; both unchanged floors passed                                      |

## Release blockers

Every applicable baseline case needs a concrete native case or a source-driven differential replay
through the native public boundary. Suite-level grouping above is a navigation index, **not all 470
cases accounted for**. Case inventory and acceptance must fail if a case is unassigned, pending,
unexecuted or relies solely on the old Node implementation. No blanket "covered" or "retired" state.

Ordinary users will need no Node, Rust or Python runtime. Node is retained only in hosted pinned-source
oracle checks; Python helpers are CI tooling, never dependencies of the shipped binary. Release and
local old-runtime cleanup wait for their separately specified verification and owner cutover gates.
