# References

## Rust completion and upstream refresh — checked 2026-11-10

Coverage follows the pinned [cargo-llvm-cov 0.9.1 external-test workflow](https://github.com/taiki-e/cargo-llvm-cov/blob/v0.9.1/README.md#get-coverage-of-external-tests):
instrument with `show-env`, execute existing workspace harnesses and inherited child processes,
then report the production package. The 85/80 floors and production source scope are unchanged.

The following current heads, releases and relevant protocol sources were read for this PR.
Large comparison responses are not exhaustive audits; conclusions refer to the specific sources
listed here. No upstream implementation was copied into the Rust workspace.

| Upstream | Inspected revision | Applied compatibility decision |
| --- | --- | --- |
| [Rust SDK](https://github.com/modelcontextprotocol/rust-sdk) | `f2b0ddf113d1ed426b77a81511eac6e4bffa0ac4`; latest release RMCP 3.9.0 | Keep the locked 3.9.0 SDK, `_meta` result fix and progress-lock correction. New stdio lifecycle documentation confirms initialization waits for host input and logs belong on stderr; actual SDK tests retain these contracts. Directory skill loading is outside the image/media server. |
| [OmniRoute](https://github.com/cheaper-inference/OmniRoute) | `895c0f3571a9265a757f27c8700a1a4b971b0092` | Update the canonical repository link. Read the current image generation handler and multipart edits dispatch. Keep version-specific explicit forwarding and no client-side paid replay; new provider routes do not prove masks or account access. |
| [9router](https://github.com/decolua/9router) | `1339107e624d1ab150c6a0c2c2ee22caa7e9db2f`, 0.5.116 | Its Antigravity adapter collects ten references and its executor now preserves `inlineData`. Add a version-scoped JSON configuration example and expand the existing Rust route test for ten encoded PNG references and eleven-reference rejection. Older default contracts remain conservative. Read the Codex native-generation / Responses-edit split without silently replacing a configured route. |
| [ComfyUI](https://github.com/Comfy-Org/ComfyUI) | `989d93d0d968e704c589ce0146bb18c77f6b50cc`, 0.45.0 | Read current `server.py` upload, history, view and interruption routes. Keep direct API graphs, exact filenames/subfolders and targeted cancellation. New models, node packages and EXR inspection are upstream features, not permission to install nodes or claim supported output codecs. |
| [Google GenAI](https://github.com/googleapis/js-genai) | `cd31e9f9ea3aac359be9666a37180b5a908b8293` | Reviewed Interactions type changes: image MIME types are open strings; image response format stays distinct from text JSON schema. Keep typed image inputs and explicit formats. Automatic text continuation and the new video CLI default do not authorize additional submissions or a model switch. |
| [OpenAI Node reference](https://github.com/openai/openai-node) | `08cc5e9a83df66a8ab7c88afce444849870b9c01`, release 7.55.0 | Read current Images request types: edits carry one or more source images; variations are retired. Keep the separate Images and Responses adapters, deterministic multipart names/MIME and explicit model IDs. No Node runtime or SDK default retry is imported. |
| [mcp-image](https://github.com/shinpr/mcp-image) | `2eade843c281ef9efc49a1a2549f9b3a50a78554`, 0.19.0 | Reviewed GPT Image 2.6 and multi-reference/MIME preflight changes. Numera already accepts explicit model IDs and MIME-derived multipart filenames; retain its per-route configurable cardinality and original preflight tests. Do not replace user models with an upstream default. |
| [comfyui-mcp](https://github.com/artokun/comfyui-mcp) | `707474d24d0d251ba04a1bebdf86620cef2eaa8b` | Reviewed bounded `/view` body-read errors and oversized-video remedies. Keep Numera's bounded reads and original bytes; do not describe image resizing as a video fix or add a fallback to unrelated local files. |
| [oh-my-pi](https://github.com/can1357/oh-my-pi) | `e1d8f29d54957da5cb38b11d10a6778d13476980` | Current host reset/routing changes remain host-owned. Numera's explicit provider endpoints, model IDs and native MCP transport remain independent; no host credential/reset behavior is imported. |
| [fal-js](https://github.com/fal-ai/fal-js) | `a29c8955d51c7e9009091b43dca4f10660f721ae` | Read reserved tag validation and endpoint type refresh. Numera does not expose `X-Fal-Tags`; fixed no-retry/store-IO headers and exact endpoint contracts stay in Rust. No SDK upload, tag passthrough or automatic POST retry is added. |

The reference branch already supersedes Cargo PRs #17–#21 with RMCP 3.9.0, rusqlite 0.41.1,
libc 0.2.191, SHA2 0.11.1 and UUID 1.29.2. Current RMCP, rusqlite and UUID release notes were
checked again. The official source-preparation run also resolved const-oid 0.10.3, crc32fast 1.5.5,
rustls-pki-types 1.16.0 and sqlite-wasm-rs 0.6.3, with registry checksums committed in Cargo.lock.
Their registry metadata preserves upstream licenses and Rust minimums below 1.99.1. The
[rustls 1.16.0 notes](https://github.com/rustls/pki-types/releases/tag/v%2F1.16.0) document trailing-dot
DNS canonicalization and rejection of hexadecimal final labels; origin-bound credential policy and
existing network tests remain required. The SQLite WASM package is a transitive lock entry, not a
new native runtime target.

Registry metadata was checked for all 25 production direct dependencies and the three additional
tooling archive dependencies. All direct pins are current stable releases except ICU collator/locale,
which intentionally remain 2.3.1. [ICU4X 2.4](https://github.com/unicode-org/icu4x/releases/tag/icu%402.4.0)
changes Unicode/CLDR collation data to Unicode 18 / CLDR 49. Upgrading that data is a separate
canonical-order/persisted-identity migration; this port preserves the frozen compatibility contract.

Latest official Actions releases remain checkout 7.0.1, upload-artifact 7.0.2 and download-artifact
8.0.2; their immutable SHA pins are retained. setup-node and npm dependency update branches are
obsolete after the maintained tooling migration. The frozen historical oracle is not upgraded.

## Rust tooling evidence, checked 2026-11-02

- [GitHub workflow jobs API](https://docs.github.com/en/rest/actions/workflow-jobs?apiVersion=2026-03-10):
  read the exact run attempt's jobs and require source, successful completion and original log receipts.
- [GitHub artifacts API](https://docs.github.com/en/rest/actions/artifacts?apiVersion=2026-03-10):
  authenticate original archive size/digest and repository/run/head identities before bounded decoding.
- [Pinned upload implementation](https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/src/shared/upload-artifact.ts)
  and its [multihost tests](https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/.github/workflows/test.yml):
  backend artifact IDs/digests and download-content assertions, not permission to copy automatic
  triggers, overwrite artifacts or delete evidence. MIT reference; no implementation copied.
- [Pinned artifact downloader](https://github.com/actions/download-artifact/blob/9000827ccba6bdab643e8b6fd33ac0654aef8333/src/download-artifact.ts)
  and [input definitions](https://github.com/actions/download-artifact/blob/9000827ccba6bdab643e8b6fd33ac0654aef8333/action.yml):
  cross-run selection requires an explicit token, repository and run; select one authenticated artifact
  ID with digest mismatches fatal. The original package-log digest and executed binary remain separate
  checks before signing. [Status expressions](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#status-check-functions)
  require explicit cancellation and prerequisite-result guards for intentionally skipped jobs.
  [Step outputs](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-commands#setting-an-output-parameter)
  use GitHub's runner-owned UTF-8 command file, not an application cache.
- [serde_json1.0.154 value indexing](https://github.com/serde-rs/json/blob/v1.0.154/README.md#operating-on-untyped-json-values)
  and [map source](https://github.com/serde-rs/json/blob/v1.0.154/src/map.rs): typed accessors and exact
  inventories precede equality checks; absent indexing yields null.
- [Cargo home](https://doc.rust-lang.org/cargo/guide/cargo-home.html) and
  [directory sources](https://doc.rust-lang.org/cargo/reference/source-replacement.html): original
  registry archives and extracted sources differ from vendor-generated checksum metadata.
  The [pinned vendor reference test](https://github.com/rust-lang/cargo/blob/809bcc525b6169903fc41aa861a2029ab7388381/tests/testsuite/vendor.rs)
  confirms that distinction. Reviewed notice supplements bind original cached crate bytes, not an
  assumed vendor file. No Cargo source copied or local compilation performed.

Source-authored receipt and execution collectors are not current hosted execution, semantic parity
or cryptographic verification. Final original archive/download verification remains a separate gate.

## Native private maintenance references, checked 2026-11-10

- [CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata):
  CurrentUser scope, UI-forbidden operation, optional entropy contract and LocalFree ownership.
- [Console modes](https://learn.microsoft.com/en-us/windows/console/setconsolemode) and
  [ReadConsole](https://learn.microsoft.com/en-us/windows/console/readconsole): echo/cooked-input behavior,
  strict Unicode input, redirected-handle limits and restoration obligations.
- [Owner of a new object](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object):
  elevated token defaults differ; private creation supplies an explicit current-user owner and DACL.
- [Pinned PowerShell serialization](https://github.com/PowerShell/PowerShell/blob/1aac7f5e38679b3c1469b67071a4c07f50cc064b/src/System.Management.Automation/security/SecureStringHelper.cs):
  CurrentUser DPAPI protects exactly UTF-16LE code units without a terminator and serializes lowercase
  hexadecimal, matching the native credential reader. Reference only; no PowerShell runtime is invoked.
- [FireDaemon OpenSSL](https://github.com/firedaemon/openssl) and
  [OpenSSL3 APIs](https://docs.openssl.org/3.5/man3/): the explicit memory adapter uses the qualified
  native library for certificate/PKCS12/signature operations, not handwritten cryptographic formats.

Source/API inspection is not interactive console, other-user access, genuine publisher signing or
hosted disposable-key execution proof. Current native maintenance remains source-authored.

## Native migration references, checked 2026-10-11

Rust1.99 stable manifest and Cargo environment reference govern hosted-only compiler/workspace cache
setup. RMCP3.5.1 official tag bc82eb861a5ae2e074183e1214dc2380ca454555 was read for ServerHandler,
legacy negotiation, tool/result/resource models and Transport; its uncapped read path is bounded by
Numera's stdio adapter. SDK Apache-2.0 notices remain required. Native runtime compatibility is not
verified merely because these sources were read.

- https://static.rust-lang.org/dist/channel-rust-stable.toml
- https://doc.rust-lang.org/cargo/reference/environment-variables.html
- https://github.com/modelcontextprotocol/rust-sdk/tree/rmcp-v3.5.1
- https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html and retry/proxy references: explicit
  no protocol retry, no inherited proxy, no API redirect, bounded credential-free assets.
- https://docs.rs/serde_stacker/0.1.14/serde_stacker/ and tagged dependency manifest: bounded deep JSON
  parse plus iterative retained clone/drop; MSRV1.67, MIT/Apache-2.0.
- https://docs.rs/base64/0.23.1/base64/engine/general_purpose/struct.GeneralPurposeConfig.html:
  compatibility decoder permits source-accepted unused trailing bits after strict syntax/size checks.
- https://github.com/taiki-e/cargo-llvm-cov/tree/v0.9.1: hosted nightly branch coverage; production
  compiler remains stable, region and branch floors are independently enforced.
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex:
  stable Win32 128-bit file identity rather than nightly-only std metadata APIs.
- https://modelcontextprotocol.io/docs/develop/build-server: host command/args launch local stdio;
  stdout must contain only protocol, and local/cloud file access is not interchangeable.

## Multimedia source contracts, checked 2026-10-29

- [fal queue API](https://docs.fal.ai/model-apis/model-endpoints/queue): native full-endpoint submission,
  owner/app/request tracking,200/202 status and exact cancellation status/body. Completed is not successful.
- [fal common headers](https://docs.fal.ai/model-apis/model-endpoints/headers): explicit no-retry and
  request-history controls do not promise private CDN retention, account entitlement or refunds.
- [Rodin API/schema](https://fal.ai/models/fal-ai/hyper3d/rodin/api): text/image roles, data-URI inputs,
  geometry format default GLB, mesh plus texture outputs. Numera accepts explicit GLB only; a provider
  supporting another format does not establish local validation support.
- [MCP2025-11-25 resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources):
  binary originals are base64 blobs with MIME, not image content for every media kind. The old image URI
  remains compatible; the new media URI is additive.

Exact endpoint bindings were compared with first-party model schemas and upstream fal-js queue source
at8086bfc4a1957e43ead8646439935b6cc25e627e. No SDK implementation copied; Rust REST deliberately avoids
SDK POST retries/automatic uploads. On2026-10-29 autonomous endpoint_id OpenAPI fetching was robots-blocked;
the allowed Rodin model API page was read instead. New source/fixtures are not current live-provider,
compiled six-platform, semantic quality, playback or rendering proof.

## Native macOS signing references, checked 2026-10-24

- [Apple memory-only PKCS12 import](https://developer.apple.com/documentation/security/ksecimporttomemoryonly),
  macOS 15+: required true flag prevents keychain persistence; missing support fails closed.
- [SecPKCS12Import](<https://developer.apple.com/documentation/security/secpkcs12import(_:_:_:)>) returns
  identities/certificates. Authenticate the actual imported certificate before using its private key.
- [Apple signing declarations](https://github.com/apple-oss-distributions/Security/blob/db15acbe6a7f257a859ad9a3bb86097bfe0679d9/OSX/libsecurity_codesigning/lib/SecCodeSigner.h)
  and implementation at the same revision define exported signing SPI, not stable public API.
  Explicit timestamp false avoids a TSA; Apple creates embedded signatures, not project-written CMS.
- [Apple's codesign signing source](https://github.com/apple-oss-distributions/security_systemkeychain/blob/218eea5c6db7b10e9c26193d16878b6a32e213c7/src/cs_sign.cpp)
  establishes the native service boundary; it is historical source, not current CLI lookup proof.
- [Code Signing In Depth](https://developer.apple.com/library/archive/technotes/tn2206/_index.html)
  distinguishes explicit certificate requirements from OS trust. Native verifier requirement source
  uses `=anchor "<authenticated-certificate>"`, not an implicitly interpreted filename.

No upstream implementation was copied. These API facts do not establish hosted signing success;
real native verification, wrong-signer/tamper rejection and extracted-byte acceptance remain gates.

## Historical protocol and Node implementation references

Checked2026-10-03. Interfaces and test patterns only; no third-party implementation copied.
Resume: SDKv2.3.0 release/protocol-versions documentation read; same direct stdio API and Apache-2.0
package licensing. Google thinking/signature and OpenRouter endpoint descriptor docs refreshed.
Current SDK/native source revisions and exact test evidence remain distinct from live availability.

| Source                                                 | Examined revision/license                              | Applied/rejected                                                              |
| ------------------------------------------------------ | ------------------------------------------------------ | ----------------------------------------------------------------------------- |
| https://github.com/modelcontextprotocol/typescript-sdk | e16d27729ea2fae144a6d957fd0372d9eb0e6c58; npm2.2.0 MIT | stable v2 split packages, Standard Schema/stdout/client tests                 |
| https://github.com/diegosouzapw/OmniRoute              | 4399e60057aaee564ec9e31054a5f6c76cc92a2d MIT           | distinct catalog/forwarding, Antigravity refs blocked                         |
| https://github.com/decolua/9router                     | a99cf57239ff778b61e434c2786009d5ed1c412c MIT           | image/info catalog, single-ref contract, terminal done; executor dropped refs |
| https://github.com/Comfy-Org/ComfyUI                   | 1b883beab11c04a2eb82cf6a3ee64294f3303897 GPL3          | API graph/upload/history/view/targeted cancel; no model/node copying          |
| https://github.com/googleapis/js-genai                 | ae573cc7f2d1d4787ac95a3fd8704afbd96c516c Apache2       | native distinct Interactions typed image/model_output steps                   |
| https://github.com/openai/openai-node                  | e39bd99f338d929c3de5a8c45a26f62e653c9523 Apache2       | Images versus Responses; reject SDK default paid retries                      |
| https://github.com/shinpr/mcp-image                    | 19e371f13bc0df25c9166aea2449f0acb65db9d7 MIT           | inspected requestBoundaries tests; reject prompt enhancement/fixed IDs        |
| https://github.com/artokun/comfyui-mcp                 | 6ad6fc0296398c41feff068369a421b94cbb285d MIT           | enqueue/upload tests and CI inspected; preserve subfolders/node_errors        |
| https://github.com/can1357/oh-my-pi                    | 47775c7a3b317aba0cb89a714342af5e89c61b71               | inspected OpenRouter wire usage, no code reuse; official resolution wins      |

Official current documentation read:

- https://openrouter.ai/docs/guides/overview/multimodal/image-generation
- https://ai.google.dev/gemini-api/docs/image-generation and /interactions
- https://ai.google.dev/api/models#method:-models.list
- https://developers.openai.com/api/docs/guides/image-generation
- https://code.claude.com/docs/en/mcp
- https://developers.openai.com/codex/mcp.md
- https://github.com/nodejs/node/blob/v24.21.0/doc/api/sqlite.md
- https://docs.comfy.org/development/comfyui-server/comms_routes

Focused GitHub issues/code/reference searches informed exact route choices; negative search results
never establish absence. OmniRoute source confirms text-only Antigravity and Codex count fanout;
9router issue2797 corroborated in executor source. Gateway-shaped fixtures are not deployed gateway
integration or live entitlement tests. Node docs autonomous fetch robots blocked, official source used.
Canonical GPL3 text fetched from pinned ComfyUI LICENSE after gnu.org connection timeout; complete text
unchanged. Popularity/coverage/benchmarks from references are not Numera facts.

Anthropic-compatible research2026-10-03: officialMessages/vision and SDKoutput ContentBlock union
https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/resources/messages/messages.ts distinguishimageinput fromoutput. OmniRoute3.8.51 buildc1e30b7/pinnedc1e30b7676975feb298b49eff6ff58923c04b89e
Messages route->chat guard and responseTranslator.ts#L736 dropimagearrays; no claimedMessagesimageadapter.
No upstreamcodecopied, no sharedgatewaymutation or additionalpaidrequest.

Terminal lifecycle sources checked2026-10-05: OpenAI ResponseStatus at
https://github.com/openai/openai-node/blob/fb6955621e1cf6653659adb75094ebace83ebfe9/src/resources/responses/responses.ts ;
Google InteractionStatusUpdateStatus at
https://github.com/googleapis/js-genai/blob/f6b85db43db2cb88705f60af9c394fee308ac497/src/gaos/models/interactions/interaction-status-update.ts ;
Comfy targeted cancellation dispatch/terminal job status at
https://github.com/Comfy-Org/ComfyUI/blob/5c460d8172fe30761ff67c0df3d5643bb74e0d70/server.py and openapi.yaml.
A true cancellation boolean confirms dispatch only, not completion/refund. SDK2.3.1 patch release
https://github.com/modelcontextprotocol/typescript-sdk/releases/tag/v2.3.1 retains stdio contracts.
No third-party implementation copied.
