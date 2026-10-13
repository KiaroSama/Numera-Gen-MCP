# Native migration acceptance matrix

Baseline: `fc7da25c3b470199949b1b4c67127d77afa0c34a`, 50 discovered suites / 470 cases.
The Rust implementation is in progress. The integrated `3655fdc` run passed all 55 native targets,
strict Clippy and actual SDK stdio/HTTP acceptance on all six OS/CPU targets, including authoritative
UTF-16/number application values, the repaired Windows crash fixture and public HTTP outputs/resources.
At `8271b81`, the retained-route/ambiguity/local-recovery regression also passed. Five stable platforms
passed all 55 runtime targets; Linux x86_64 exceeded the aggregate asynchronous polling fixture's
eight-second deadline. All six passed strict Clippy and actual SDK stdio/HTTP acceptance. Formatting
failed in one test file. Complete instrumented coverage measured 84.72% regions / 81.17% branches:
the region floor still failed against unchanged 85% / 80% gates.

**Packaged execution and complete baseline-case migration acceptance have not passed.** Passing
selected JSON integration fixtures is not full accepted-domain or 470-case parity. Old TypeScript
production source remains authoritative until hosted differential verification passes. A file
existing is not a verified mapping. Authored polling-case isolation and Comfy upload/preflight/recovery
regressions require the replacement hosted run; they establish no new passing claim here.

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
| Native archives/hash/extraction/licenses/docs         | `scripts/native-package.py`                                           | Distribution checks not yet complete                              | Pending verified downloadable archives                                                                                    |
| Coverage                                              | Hosted nightly only                                                   | LLVM regions >=85%, branches >=80%                                | Latest complete report: 84.72% regions / 81.17% branches; regions failed; no reduced floor or exclusions                  |

## Release blockers

Every applicable baseline case needs a concrete native case or a source-driven differential replay
through the native public boundary. Suite-level grouping above is a navigation index, **not all 470
cases accounted for**. Case inventory and acceptance must fail if a case is unassigned, pending,
unexecuted or relies solely on the old Node implementation. No blanket "covered" or "retired" state.

Ordinary users will need no Node, Rust or Python runtime. Node is retained only in hosted pinned-source
oracle checks; Python helpers are CI tooling, never dependencies of the shipped binary. Release and
local old-runtime cleanup wait for their separately specified verification and owner cutover gates.
