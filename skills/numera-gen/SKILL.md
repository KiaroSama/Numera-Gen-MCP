---
name: numera-gen
description: Generate or edit images, videos, audio or 3D through an installed Numera Gen MCP connection; select explicit model modes, verify originals and recover existing jobs without duplicate submission.
---

# Numera Gen MCP workflow

Use the installed server's actual tool schemas. The signed 0.2.0 release has ten image tools; the
0.3.0 source candidate adds media tools but is not yet qualified or installed. The server works
without this optional skill. Loading instructions grants no provider, filesystem or cost permission.

1. Call list_connections, list_models and get_model_capabilities for the selected destination/model.
   Separate configured outputTypes/modes, documented capability, implemented forwarding and account
   entitlement. Proceed only when the exact operation and required input roles are supported.
2. Establish user authorization for the operation, destination, approved references and request/cost
   ceiling. Keep the host chat model unchanged; send private inputs only to the approved destination.
3. Keep one request_id for the logical operation and its effective inputs. For images use
   generate_image/edit_image with typed reference_images and explicit native settings. If the actual
   server exposes generate_media, supply its exact model/mode, parameters and typed references with
   kind/role/source. Unsupported modes/formats/ref/mask forwarding must refuse, not drop inputs or
   silently convert/upload.
4. Submit once. After a timeout, disconnect, running or outcome_unknown receipt, inspect get_job with
   refresh=false first. Refresh only the existing owned job when needed; an unknown outcome never
   authorizes resubmission with a fresh ID, paid fallback or changed provider. cancel_job distinguishes
   local waiting from upstream cancellation and refund; a new submission needs new authorization.
5. Inspect receipt status, outputs, deviations and errors. Use get_output_info for images or the exposed
   get_media_output_info for typed media originals. Report actual MIME, bytes/hash and available format
   metadata; preserve originals. Structural validation proves neither semantic quality nor successful
   playback/rendering. Deliver owned files/resources; preview/local-path access depends on the host.

## Image-specific edits

For authorized watermark removal, preserve the original and use edit_image with the precise prompt,
approved reference_images and edit_region={x,y,width,height} around the mark on the first image.
Verify actual dimensions and native mask support before submission; never guess coordinates or change
the selected provider. Inspect reconstruction quality. Modify only images the user owns or may edit.
For comic translation, target_language requests replacement inside the image, not separate text output.

## Media limits

Explicit output kinds are image, video, audio and 3d; a prompt/transcript is not a binary output kind.
Use only the installed endpoint's implemented transformations. A declared mode is not a universal
conversion promise. Original-byte validation currently targets PNG/JPEG/WebP, bounded MP4, WAV and GLB
subsets; another provider format is not automatically supported. Models may return several roles,
including textures/thumbnails; account for saved and failed items rather than calling partial complete.
Keep credentials out of arguments and follow configured input roots and host permissions. Source-only
examples and successful health/catalog checks do not prove live generation, quality or GPU execution.
