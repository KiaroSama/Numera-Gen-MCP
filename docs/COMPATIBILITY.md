# Compatibility evidence

## Numera Gen MCP 0.3.0 rebrand boundary

The current source uses product **Numera Gen MCP**, executable `numera-gen-mcp`, Rust crate
`numera_gen_mcp`, host entry `numera-gen-mcp` and repository `KiaroSama/Numera-Gen-MCP`. It is a new
candidate: historical 0.2.0 CI, signatures and downloads do not qualify 0.3.0 until its own hosted
checks pass. Released 0.2.0 archive names, embedded executable/product metadata, signed manifests,
certificate/key bytes and source hashes remain unchanged.

Existing `NUMERA_*` keys, adjacent `.env`/`config.local.json`, Windows `Numera/ImageGen` and Unix
`numera-image-gen` per-user roots remain compatibility addresses; private files are not relocated.
Signed old installations retain their distribution-owned executable names. Current registration
writes only `numera-gen-mcp`; it never silently renames/deletes an existing `numera-image-gen` entry.
Back up and explicitly migrate the selected entry before reconnecting; do not run duplicate entries
against one receipt/state directory. Legacy `numera-image://outputs/` addresses remain readable.
Old image request fingerprints, original output identifiers/bytes and unknown-outcome receipts are
not rewritten by this rebrand. Typed media declarations are additive, not universal model support.

## Historical image evidence

Protocol compatibility is not a measured percentage of all models. Model names do not prove support.
Every live connection/model/account/size/input mode must be verified separately. The published baseline
and resumed acceptance additions have separate offline evidence. Compact config covers generic Images, explicit OmniRoute/9router and Responses with a separate
orchestration_model; other native protocols use advanced config. Linux do not require
DPAPI. Version 0.4.0 removes macOS; the hosted matrix covers Windows and Linux x64/ARM64.
Exact versions, coverage and remaining live limitations belong in TEST-RESULTS.md.
The recovery/model-limit changes added on2026-10-05 passed a fresh hosted three-platform matrix
on320f903, including original-byte normal/recovered comparison, real child interruption and packed
installation fixtures. See TEST-RESULTS.md for exact counts and coverage; this is not live-provider proof.
Canonical Pro/preview dimension evidence describes the public model, not Antigravity account access;
the separately repaired gateway still has no verified successful Pro generation.

| Profile                | Generation                           | Reference/edit route                        | Evidence/limits                                                             |
| ---------------------- | ------------------------------------ | ------------------------------------------- | --------------------------------------------------------------------------- |
| Generic Images         | JSON Images                          | multipart or explicit JSON/generation input | exact fixture mapping; capabilities configured per route                    |
| OmniRoute              | Images JSON                          | supported custom/Codex edits                | pinned source; Antigravity text-only, masks unsupported, Codex n>1 rejected |
| 9router                | Images JSON/binary/SSE parse         | selected verified single-ref route          | pinned source; Antigravity executor drops refs so rejected                  |
| Gemini generateContent | contents/inlineData                  | native references                           | native protocol, thought images excluded; mask/format not universal         |
| Gemini Interactions    | typed input/response_format          | native references                           | separate steps model_output, stateless default                              |
| OpenAI Responses       | image tool under orchestration model | input_image                                 | completed call results only; mask requires other documented route           |
| OpenRouter Images      | dedicated /images                    | input_references                            | dedicated model/endpoints descriptors; no automatic fallback                |
| Chat images            | opt-in chat/completions              | documented image_url input                  | explicit image output only; never gateway image-only models                 |
| ComfyUI                | explicit API graph                   | explicit reference/mask graph bindings      | node/GPU checks; job history/view, no automatic node/checkpoint install     |

Native count>1 requires documented maxCount and actual mapped batch support; not silently repeated.
PNG/JPEG/WebP/GIF originals validated. GIF (`GIF87a`/`GIF89a`) is the only animated container accepted
and multi-frame animated GIF bytes are preserved untouched. On OpenAI-compatible image routes that
support GIF output (such as `gpt-image-2.6-sunburst` and `gpt-image-2.6-flare`), `generate_image` and
`edit_image` forward `output_format: "gif"`, `frame_count` (`2`-`16`), and `frame_duration_ms`
(`20`-`2000`) with `size: "auto"` or multiples of 16; multi-frame APNG and animated WebP remain
rejected, and SVG is not decoded. For typed media (`generate_media`), 1.0.0 validates WAV (PCM,
IEEE-float, and `WAVE_FORMAT_EXTENSIBLE`), self-contained non-fragmented MP4 (visual track plus
optional `mp4a` AAC or `Opus` audio track, including `fal-ai/mmaudio-v2` `video-to-audio` outputs),
and self-contained GLB 2.0 meshes (including safe Khronos material/texture extensions). Source
references, live/GUI/GPU not-run reasons and exact runtime/platform evidence are separate. SDK 2.3.0
selected. Local tests verify legacy
initialize and auto negotiation falls back to legacy on this direct stdio server; a modern-only pin is
rejected. No modern-only transport support is advertised. Provider signatures are privately retained;
stateless file-reference editing is supported, conversational continuation replay is not exposed.

### OpenAI-compatible vs Anthropic-compatible API

Numera currently submits images through evidenced image APIs, including OpenAI-compatible Images
and Responses. Using Claude as the MCP host does not require an Anthropic image endpoint: Claude
calls Numera over MCP, and Numera calls your selected image backend independently.

The standard Anthropic POST /v1/messages contract accepts images for vision but does not define a
native generated-image output or OpenAI-style image_generation tool. A generic Anthropic-compatible
chat endpoint is therefore not sufficient for image generation. Numera does not claim such an adapter.

The examined OmniRoute3.8.51 Messages path delegates to chat; image-only models are rejected and its
Claude response translation does not preserve generated image content. Use its supported
/v1/images/generations route instead. A future Messages image-output adapter needs an explicit
gateway extension defining request options, authentication and final image bytes; no protocol
inference, silent parameter dropping or automatic fallback is permitted.

Sources: [Anthropic Messages](https://platform.claude.com/docs/en/api/messages/create),
[vision input](https://platform.claude.com/docs/en/build-with-claude/vision), and
[pinned OmniRoute response conversion](https://github.com/diegosouzapw/OmniRoute/blob/c1e30b7676975feb298b49eff6ff58923c04b89e/open-sse/handlers/responseTranslator.ts#L736).
