# VideoForge v2.0

VideoForge is a deterministic, production-grade GPU video inference engine written in Rust. It performs super-resolution upscaling using NVDEC hardware decoding, TensorRT/ONNX Runtime inference, and NVENC hardware encoding with GPU-resident frame transport — no CPU frame copies in steady-state operation.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          VideoForge v2.0 Pipeline                           │
│                                                                              │
│  ┌──────────┐  ch(4)  ┌────────────┐  ch(2)  ┌───────────┐  ch(4)  ┌──────────┐
│  │ Decoder  │────────►│ Preprocess │────────►│ Inference │────────►│ Encoder  │
│  │ (NVDEC)  │         │(CUDA kern) │         │(TensorRT) │         │ (NVENC)  │
│  │ NV12 GPU │         │NV12→RGB F32│         │RGB F32/16 │         │NV12 GPU  │
│  └──────────┘         └────────────┘         └───────────┘         └──────────┘
│                                                                              │
│  ═══════════ GPU-resident frame transport · no CPU frame copies ════════════│
└──────────────────────────────────────────────────────────────────────────────┘
```

## Why v2.0 exists

Typical video super-resolution pipelines suffer from four structural bottlenecks:

1. **Host↔device copies**: FFmpeg decodes on the CPU, frames are uploaded to GPU for inference, then downloaded back for encoding. Each 4K frame round-trip burns ~12 MB of PCIe bandwidth.
2. **Python GIL contention**: Python-based inference wrappers serialize GPU work behind the GIL, preventing true pipeline overlap between decode, inference, and encode.
3. **Unbounded VRAM growth**: Without backpressure, a fast decoder fills GPU memory with queued frames until OOM. Python pipelines rarely enforce memory ceilings.
4. **Non-deterministic threading**: Subprocess pipelines (FFmpeg | Python | FFmpeg) have no shared cancellation, no structured error propagation, and non-deterministic frame ordering under load.

VideoForge v2.0 eliminates these by keeping frames in GPU device memory from NVDEC decode through TensorRT inference to NVENC encode, orchestrated by a bounded async Rust pipeline with RAII resource management and structured telemetry. No Python runtime. No subprocess I/O.

## Performance snapshot

Illustrative numbers on an RTX 4090, 1080p H.265 input, 4× Real-ESRGAN model:

| Metric | FP32 | FP16 |
|--------|------|------|
| Throughput | ~4–6 fps | ~8–12 fps |
| Inference latency (avg) | ~120 ms/frame | ~60 ms/frame |
| Steady-state VRAM | ~2.8 GB | ~1.9 GB |
| Pool hit rate (after warm-up) | >99% | >99% |
| CPU frame copies | 0 | 0 |

Throughput is model-bound. Decode and encode stages overlap inference via bounded channel backpressure. Actual numbers vary by model architecture, input resolution, and GPU.

## Features

- **NVDEC hardware decoding** — H.264 and HEVC bitstreams decoded directly to GPU NV12 surfaces via NVIDIA Video Codec SDK
- **CUDA preprocessing kernels** — NV12 → RGB F32/F16 planar conversion compiled at runtime via NVRTC (cudarc)
- **TensorRT inference via ONNX Runtime** — models execute through the ORT TensorRT execution provider with IO binding for device-resident tensor handoff
- **NVENC hardware encoding** — upscaled frames encoded to H.264/HEVC on the GPU encoder ASIC without host staging
- **GPU-resident frame transport** — frame data stays in device memory across all four stages; no `cudaMemcpy` in steady state
- **Bucketed buffer pool** — RAII-tracked VRAM recycling pool keyed by allocation size; zero CUDA driver allocations after warm-up
- **Bounded backpressure** — four concurrent stages connected by `tokio::sync::mpsc` channels (capacity 4/2/4); a slow stage stalls its upstream producer rather than allowing unbounded VRAM growth
- **Micro-batch support** — configurable `BatchConfig` with `max_batch` and `latency_deadline_us` (default: single-frame, 8 ms deadline) for trading latency against throughput
- **FP16/FP32 precision control** — per-model precision selection for balancing quality vs. throughput
- **Container and raw bitstream I/O** — MP4/MKV/MOV/AVI/WebM containers via FFmpeg FFI, or raw Annex B bitstreams
- **VRAM accounting with hard limits** — atomic byte counter with optional `--vram-limit` ceiling; exceeding the limit returns `EngineError::VramExhausted`
- **Structured telemetry** — per-stage latency, queue depth, pool hit/miss rates, and inference peak/avg timing via lock-free atomic counters and `tracing`
- **Deterministic shutdown** — `tokio_util::CancellationToken` propagates Ctrl+C to all stages; EOS is signaled by channel closure, not sentinel values

## Architecture

VideoForge is organized into six modules (~8K lines of Rust):

```
videoforge-engine
├── core/           GPU contract types, shared context, buffer pool, CUDA kernels
│   ├── types       GpuTexture, FrameEnvelope, PixelFormat
│   ├── context     GpuContext, BucketedPool, VRAM accounting, QueueDepthTracker
│   ├── kernels     PreprocessKernels (NV12↔RGB), NVRTC compilation, StageMetrics
│   └── backend     UpscaleBackend trait, ModelMetadata
├── backends/       Inference backend implementations
│   └── tensorrt    TensorRtBackend, ORT session, IO binding, OutputRing, BatchConfig
├── codecs/         Hardware codec wrappers
│   ├── sys         Raw FFI bindings (nvcuvid, nvEncodeAPI, CUDA driver)
│   ├── nvdec       NvDecoder, BitstreamSource trait, parser callbacks
│   └── nvenc       NvEncoder, BitstreamSink trait, registration cache
├── engine/         Pipeline orchestration
│   ├── pipeline    UpscalePipeline, PipelineConfig, PipelineMetrics
│   └── inference   InferencePipeline (end-to-end convenience wrapper)
├── io/             Container and file I/O
│   ├── probe       FFmpeg container probing (codec, resolution, framerate)
│   ├── ffmpeg_demuxer / ffmpeg_muxer    Container demux/mux via FFmpeg FFI
│   ├── file_source / file_sink          Raw bitstream I/O
│   └── ffmpeg_sys  FFmpeg constant/type re-exports
└── error           EngineError, stable numeric error codes, Result alias
```

Data flows through the pipeline as:

1. **Decode** — NVDEC outputs NV12 surfaces (Y + UV semi-planar, 4:2:0) into GPU memory
2. **Preprocess** — CUDA kernel converts NV12 → RGB F32 planar (NCHW, normalized [0,1])
3. **Inference** — TensorRT executes the super-resolution model, outputting upscaled RGB F32 planar
4. **Postprocess + Encode** — CUDA kernel converts RGB F32 → NV12, NVENC encodes to H.264/HEVC bitstream

Backpressure flows in the opposite direction: when the encoder cannot keep up, its input channel fills (capacity 4), which stalls the inference stage's `.send().await`, which in turn fills the preprocess→inference channel (capacity 2), and so on back to the decoder. This bounds total in-flight VRAM to `sum(channel_capacities) × frame_size`.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full design document including memory model, concurrency model, and cross-stream synchronization.

## Quick start

### Prerequisites

- **NVIDIA GPU** with NVDEC/NVENC support (Turing or newer recommended)
- **CUDA Toolkit 12.x** (driver API; cudarc's `cuda-12060` feature is backwards-compatible)
- **NVIDIA Video Codec SDK** headers and libs (`nvcuvid`, `nvEncodeAPI`)
- **FFmpeg 7.x** shared libraries (`avcodec`, `avformat`, `avutil`) + headers
- **ONNX Runtime 1.20+** with TensorRT execution provider
- **Rust 1.85+** (edition 2024)
- **An ONNX super-resolution model** (e.g., Real-ESRGAN, BSRGAN)

### Build

```bash
git clone https://github.com/CalvinSturm/VideoForgeV2.0.git
cd VideoForgeV2.0/engine-v2
cargo build --release
```

### Run

```bash
# Container input/output (auto-detected by extension)
./target/release/videoforge -i input.mp4 -o output.mp4 -m model.onnx

# FP16 precision on device 0
./target/release/videoforge -i input.mp4 -o output.mkv -m model.onnx -p fp16 -d 0

# Raw bitstream (requires explicit codec/resolution)
./target/release/videoforge -i input.265 -o output.265 -m model.onnx --codec hevc --width 1920 --height 1080

# With VRAM ceiling
./target/release/videoforge -i input.mp4 -o output.mp4 -m model.onnx --vram-limit 4096
```

### CLI flags

| Flag | Description |
|------|-------------|
| `-i`, `--input` | Input video file (container or raw bitstream) |
| `-o`, `--output` | Output video file |
| `-m`, `--model` | Path to ONNX super-resolution model |
| `-p`, `--precision` | Model precision: `fp32` or `fp16` (default: `fp32`) |
| `-d`, `--device` | CUDA device index (default: `0`) |
| `--codec` | Codec for raw bitstreams: `h264` or `hevc` |
| `--width`, `--height` | Input resolution (required for raw bitstreams) |
| `--vram-limit` | Maximum VRAM usage in MB |

## Project status

**What works:**
- End-to-end GPU-resident upscaling pipeline (NVDEC → TensorRT → NVENC)
- Container demux/mux via FFmpeg (MP4, MKV, MOV, AVI, WebM)
- Raw H.264/HEVC bitstream I/O
- FP16 and FP32 model inference
- Bucketed buffer pool with zero steady-state allocations
- Bounded backpressure across all pipeline stages
- Per-stage telemetry (latency, queue depth, pool stats)
- Graceful shutdown via Ctrl+C (tokio CancellationToken)

**What's next:**
- Multi-GPU scaling (distribute frames across devices)
- Batch inference (process N frames per TensorRT invocation)
- Additional model architectures beyond super-resolution
- Linux build and CI/CD pipeline

## License

MIT License — see [LICENSE](LICENSE) for details.
