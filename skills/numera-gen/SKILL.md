---
name: numera-gen
description: Generate or edit still images, animated GIFs, videos, audio, video soundtracks or 3D meshes through an installed Numera Gen MCP connection; select explicit model modes, verify originals and recover existing jobs without duplicate submission.
---
# Numera Gen MCP workflow

Use the installed server's actual tool schemas. Version 1.0.0 exposes 13 tools: `health_check`,
`list_connections`, `list_models`, `get_model_capabilities`, `generate_image`, `edit_image`,
`get_job`, `list_jobs`, `cancel_job`, `list_outputs`, `get_output_info`, `generate_media`, and
`get_media_output_info`. Historical releases expose fewer tools (`0.2.0` has ten image tools;
`0.3.0` to `0.5.0` have twelve). The server works without this optional skill. Loading these
instructions grants no provider, filesystem, or cost permission.

1. Call `list_connections`, `list_models`, and `get_model_capabilities` for the selected destination
   and model. Separate configured `outputTypes` and `modes`, documented capability, implemented
   forwarding, and account entitlement. Proceed only when the exact operation and required input
   roles are supported.
2. Establish user authorization for the operation, destination, approved references, and cost ceiling.
   Keep the host chat model unchanged and send private inputs only to the approved destination.
3. Keep one `request_id` per logical operation and its effective inputs:
   - Still images and animated GIFs: call `generate_image` or `edit_image` with explicit native
     settings and typed `reference_images` (`output_id`, `path`, `url`, or `data_url`).
   - Video, audio, video-to-audio, and 3D: call `generate_media` with the exact `model`, `mode`,
     endpoint `parameters`, and typed `references` carrying `kind`, `role`, and one source.
   - Long-running operations emit MCP `notifications/progress` updates when the client supplies a
     `progressToken`. `openai-responses` and `gemini-interactions` also accept `wait: false` for
     non-blocking background job creation while keeping `store: false`.
   - Unsupported modes, formats, references, or mask forwarding must refuse before submission rather
     than dropping inputs or silently converting or uploading.
4. Submit once. After a timeout, disconnect, `running`, or `outcome_unknown` receipt, inspect
   `list_jobs` or `get_job` with `refresh: false` first, then refresh only the existing owned job.
   An unknown outcome never authorizes resubmission with a fresh ID, paid fallback, or a different
   provider. `cancel_job` separates stopping local waiting from upstream cancellation or refund; any
   new submission requires fresh authorization.
5. Inspect receipt `status`, `outputs`, `deviations`, and `errors`. Call `get_output_info` for images
   and animated GIFs, or `get_media_output_info` for typed media originals. Report actual MIME, bytes,
   SHA-256, and format metadata. Originals are preserved byte-for-byte without resizing or re-encoding.
   Structural validation does not prove semantic quality, smooth animation, or playback rendering.

## Animated GIF generation and editing

Models that support OpenAI animated GIF output (such as `gpt-image-2.6-sunburst` and
`gpt-image-2.6-flare`) accept `output_format: "gif"` on both `generate_image` and `edit_image`:

- `frame_count`: optional integer `2` to `16` (upstream default `9`).
- `frame_duration_ms`: optional integer `20` to `2000` milliseconds per frame (upstream default `200`,
  rounded to 10 ms).
- `size`: `"auto"` or `WIDTHxHEIGHT` with dimensions divisible by 16 within the model's GIF pixel
  budget.
- Do not combine `output_format: "gif"` with streaming, partial images, masks, or `output_compression`.
- Multi-frame `GIF87a` and `GIF89a` bytes are preserved untouched. If an upstream or gateway drops
  `output_format` and returns PNG instead, Numera saves the original bytes, marks the receipt
  `partial`, and records the format deviation rather than relabeling the file as GIF.

## Image-specific edits

- Watermark removal or local region edits: preserve the original and call `edit_image` with a clear
  prompt, approved `reference_images`, and `edit_region: {x, y, width, height}` on the first image.
  Verify actual dimensions and native mask support before submission; never guess coordinates or
  switch providers. Edit only images the user owns or has permission to modify.
- Comic translation: pass `target_language` to request in-image text replacement rather than separate
  chat text.
- Model-based upscaling: pass `upscale: true` with one approved reference and a larger numeric `size`
  on a route that supports both references and that native size.

## Supported media modes and format limits

Explicit output kinds are `image`, `video`, `audio`, and `3d` (text prompts and transcripts are not
binary output kinds). Built-in `fal-queue` endpoints (direct `https://queue.fal.run` or via an
OmniRoute `/v1/fal` gateway) and opt-in `modelOverrides.<id>.falContract` definitions cover all 11
modes:

- Image: `text-to-image` (`fal-ai/flux/dev`), `image-to-image` (`fal-ai/flux/dev/image-to-image`)
- Video: `text-to-video` (`fal-ai/kling-video/v2.5-turbo/pro/text-to-video`), `image-to-video`
  (`fal-ai/kling-video/v2.1/standard/image-to-video`), `video-to-video` (`fal-ai/cogvideox-5b/video-to-video`,
  `fal-ai/mmaudio-v2`)
- Audio and soundtrack: `text-to-audio` (`fal-ai/stable-audio`), `audio-to-audio`
  (`fal-ai/stable-audio-25/audio-to-audio`), `video-to-audio` (`fal-ai/mmaudio-v2`, producing a muxed
  MP4 with video and audio tracks)
- 3D: `text-to-3d` (`fal-ai/hunyuan3d-v3/text-to-3d`, `fal-ai/hyper3d/rodin`), `image-to-3d`
  (`fal-ai/trellis`, `fal-ai/hyper3d/rodin`), `3d-to-3d` (`fal-ai/trellis-2/retexture`)

Original-byte validation covers PNG, JPEG, WebP, single-frame and animated `GIF87a`/`GIF89a`, WAV
(PCM, IEEE-float, and `WAVE_FORMAT_EXTENSIBLE`), self-contained non-fragmented MP4 (visual track plus
optional `mp4a` AAC or `Opus` audio track), and self-contained GLB 2.0 meshes (including safe Khronos
material/texture extensions). Models may return multiple roles (such as meshes plus preview images or
textures); account for every saved and failed item rather than reporting `partial` as complete. Keep
credentials out of tool arguments, respect configured input roots, and do not equate catalog or health
checks with live generation or GPU execution.
