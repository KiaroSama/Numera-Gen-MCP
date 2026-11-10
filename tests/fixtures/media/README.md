# Structural media fixtures

The original triangle, PCM samples, generated PNG/JPEG and unfragmented JPEG-in-MP4 fixtures
remain unchanged. Existing contract tests also construct small RIFF/WAVE variants, glTF 2 binary
geometry/textures/node graphs and ISO BMFF AVC/HEVC configuration/sample-table variants in memory.
These are independent format fixtures; no production parser code or provider implementation is
copied into the fixture builders. AVC/HEVC fixtures establish structural validation, not playback,
decoding quality or a live model result.

The same test functions exercise every truncation and bounded single-byte mutations, malformed
native parameters and explicit resource/shape/cardinality boundaries. No test function is removed
or skipped and no coverage exclusion is introduced.
