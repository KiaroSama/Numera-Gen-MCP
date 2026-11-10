# Third-party notices

First-party Numera code is GPL-3.0-only. No reference implementation source copied.

## Native distribution

The published 0.2.0 archives include their verified resolved license inventory; see
[the exact release evidence](docs/releases/0.2.0.md). Current 0.3.0 source is a separate, unqualified
candidate: successful hosted [preparation42711139081](https://github.com/KiaroSama/Numera-Gen-MCP/actions/runs/42711139081)
on `370422d` returned a candidate Cargo.lock and Rust formatting patch. Current manifests select
RMCP3.9.0, UUID1.29.2, flate2 1.1.11 and ZIP9.5.0. Preparation is not runtime or license qualification;
a complete resolved transitive license inventory is still required before distributing that candidate. Every ready archive must
include the actual resolved dependency licenses/notices and bundled SQLite/codec notices; this document
is not a substitute. RMCP, Tokio, reqwest, rusqlite, serde/serde_json, ICU4X, image, Base64, SHA2, chrono,
regex, UUID, URL, ring and the tooling archive crates retain their upstream licenses.
The authoritative JSON implementation uses no embedded JavaScript runtime. Removed validation/parser
helpers are not native runtime dependencies; the actual archive license inventory follows the resolved
lockfile, not a historical list.
The published `alloc-stdlib 0.3.0` crate omits its BSD-3-Clause notice. The distribution preserves
[the original upstream notice](docs/licenses/alloc-stdlib-0.3.0/LICENSE) from the archive's exact
source revision. [Supplement provenance](docs/licenses/supplements.json) pins the crate checksum,
source revision and notice hash; an unknown missing notice or mismatched supplement fails packaging.
The published `rmcp 3.5.1` crate also omits its repository-root license. Its
[complete original notice](docs/licenses/rmcp-3.5.1/LICENSE), including Apache-2.0/MIT transition
terms and the documentation attribution statement, is preserved verbatim from the archive's source
revision. The package's declared license and the original notice are recorded separately, not
collapsed into a generic Apache text. The inspected published `rmcp 3.9.0` crate is 677294 bytes
with SHA-256 `d24a7e92f4bcaa36b97fa93c4fa7ca8f3e669eced949345b3462dba333c7f909`, matching
its registry checksum and the prepared lockfile. Its packaged `LICENSE` is 12227 bytes with SHA-256
`0382b0057770ca05e9c350a50aa3b1c1fea84da0bc81d723bf00b9aa841be58a`; it retains the Apache-2.0
transition, MIT contributions and CC BY 4.0 documentation attribution. Preserve that complete text,
not just the declared Apache-2.0 label. The crate records source revision
`5596a8a0bba2a1ec5519cc855cdadf23e4dcecdc`, path `crates/rmcp`, and contains no separate `NOTICE`.
This byte inspection is not an executed license-collection result. The `3.5.1` supplement remains
historical evidence, not a replacement for the current crate's own license.
The source-side Google dimension table is documented under CC BY 4.0 attribution in the source and
references. Image/model/asset rights remain separate from the software license.

## Native identity test dependency

Windows hosted identity contracts qualify FireDaemon OpenSSL3.5.9 from its fixed upstream archive,
then select only the exact x64 or ARM64 `libcrypto-3` DLL by size, SHA-256 and PE architecture.
OpenSSL is Apache-2.0; retain the upstream distribution's applicable license/notice terms if those
bytes are distributed. This hosted test dependency is not a bundled MCP runtime dependency, a
publisher credential or a local installation. Current source adds no crypto implementation crate and
copies no upstream implementation. Original package: [FireDaemon OpenSSL](https://github.com/firedaemon/openssl).
Actual hosted loading and disposable-key tests remain unqualified until their own run succeeds.

## Historical Node implementation / hosted source oracle

The pinned source oracle uses these existing dependency packages only in CI. Their notices remain
required while those packages are distributed; they are not user prerequisites for a native binary.
Runtime dependencies retain their licenses/notices in distributed npm dependency packages:

| Dependency                   | Locked version | License/source                                                            |
| ---------------------------- | -------------- | ------------------------------------------------------------------------- |
| @modelcontextprotocol/server | 2.3.1          | Apache-2.0, modelcontextprotocol/typescript-sdk                           |
| zod                          | 4.6.5          | MIT, colinhacks/zod                                                       |
| sharp                        | 0.35.5         | Apache-2.0, lovell/sharp; bundled libvips and codecs retain their notices |
| undici                       | 8.11.2         | MIT, nodejs/undici                                                        |

Build/test dependencies include TypeScript7.0.2 native compiler and the official
@typescript/typescript6 6.0.2 compatibility package exposing TypeScript6.0.3 API (Apache-2.0),
Vitest/coverage-v8 5.0.3 MIT,
MCP client2.3.1 Apache-2.0, ESLint10.12.0 MIT, typescript-eslint8.71.1 MIT, Prettier3.9.9 MIT.
package-lock.json records resolved versions/integrity. Preserve upstream package license files and
bundled native notices when distributing. Code license does not establish rights to generated images,
input photographs, models/checkpoints or the owner's logo; provider/asset terms remain separate.
