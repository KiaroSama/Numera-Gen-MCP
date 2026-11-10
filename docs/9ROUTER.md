# 9router profile

## Current version review — 2026-11-10

Version 0.5.116 at `1339107e624d1ab150c6a0c2c2ee22caa7e9db2f` now preserves Antigravity
reference `inlineData` in its executor. Its image adapter collects up to ten references from
`images`/`reference_images` or their scalar equivalents. Use
[the version-scoped example](../examples/9router-current.json) only after confirming that gateway
version and the exact model route. It explicitly enables JSON generation with up to ten references,
one submission and no masks. Replace its example directories with absolute paths for your OS and
set your own `NINEROUTER_API_KEY`. A Rust route contract checks the encoded PNG inputs and limit.
The default historical profile remains conservative for older installations; its legacy limitation
text describes the unconfigured default. A configured forwarding claim does not establish account
entitlement or a successful live-provider call. The current Codex adapter separates native image
generation from Responses edits; no automatic switch of the configured route is introduced.

Sources: [adapter](https://github.com/decolua/9router/blob/1339107e624d1ab150c6a0c2c2ee22caa7e9db2f/open-sse/handlers/imageProviders/antigravity.js),
[executor](https://github.com/decolua/9router/blob/1339107e624d1ab150c6a0c2c2ee22caa7e9db2f/open-sse/executors/antigravity.js).

## Historical profile

Distinct gateway profile, not renamed OmniRoute. Disabled example20128 is not proof of a running
service. Set its own address/auth/key. Examined revision a99cf57239ff778b61e434c2786009d5ed1c412c.

Read models/image and model info?id=opaque-id. Generation images/generations has JSON outputs;
optional documented binary query returns only first image and may fall back JSON. Its Codex SSE
uses Accept:text/event-stream and terminal named done/error, not universal [DONE]. Partial images
and clean EOF alone are not final results.

Antigravity adapter accepts image or images[0], but the examined executor strips inlineData.
Numera rejects those reference operations instead of silently degrading to text generation. Corrected
future route support needs version-scoped evidence; adding an MCP parameter does not repair forwarding.
Codex profile supports a verified single-reference generation input; additional references require a
separately configured and tested cardinality. No arbitrary multiple-reference claim.

9router's ComfyUI adapter at this revision is a placeholder prompt-string path, not a correct API graph.
Use Numera's explicit direct ComfyUI backend. Do not infer masks/quality/tiers/streaming from model names.
Inspect upstream route limits and internal fallbacks separately. [REFERENCES.md](REFERENCES.md).
