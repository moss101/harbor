Synthetic test media for the multimodal embedding live tests (decision 0015):
three generated images (rendered by `tools`-free CoreGraphics: an invoice with
text, a sky/grass landscape, a bar chart) and two macOS `say` speech clips
(16 kHz mono WAV). Original content, no third-party licence.

Video fixtures: `chart_growth.mp4` and `landscape_sun.mp4` (AVAssetWriter,
2 fps, 4 s) and their sampled frames `*_frame{0..3}.jpg`, produced by the
same AVFoundation sampler the macOS/iOS app uses (exact seeking, evenly
spaced). The frames are checked in so the core test needs no video decoder.
