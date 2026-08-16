# HyperThree Native roadmap

## Runtime

- [x] Native Rust host and wgpu surface prototype
- [x] AI-friendly project manifest and Vite bundle import path
- [x] JavaScript-to-native camera/mesh/material bridge
- [ ] macOS Metal, Windows DirectX 12/Vulkan, Linux Vulkan/software CI matrix
- [x] Cross-platform CI skeleton and headless GPU backend diagnostics
- [x] Native keyboard input, frame clock, and start/update/stop lifecycle callbacks
- [x] Native mouse position/button input and perspective/orthographic camera sync
- [x] Project-relative mmap asset API and glTF/GLB metadata inspection
- [x] Native glTF/GLB POSITION/index decode and cached GPU geometry upload
- [x] Boa ES module execution with relative and `node_modules` resolution
- [x] Native `performance.now()` and requestAnimationFrame compatibility loop
- [x] Native Web Audio decode/playback bridge, AudioContext/Gain/Panner/BiquadFilter graph, dynamic filter parameter propagation, playback-rate/detune, spatial sink position, and Blob/File/object-URL asset boundary
- [x] Native AudioAnalyser FFT/time-domain tap, AudioContext-close cleanup, and Three.js AudioAnalyser fixture
- [x] Project-sandboxed localStorage/sessionStorage with restart persistence
- [x] Origin-private File System Access API slice (navigator.storage.getDirectory, file handles, writable streams, listing, removal)
- [ ] Cross-platform audio-device/spatial-DSP validation
- [ ] Embedded V8 isolate and module/runtime compatibility
- [x] Three.js scene sync for BoxGeometry, PlaneGeometry, and SphereGeometry
- [x] Three.js position/index BufferGeometry registration and native GPU draw path
- [x] BufferGeometry UV attributes and glTF base-color RGBA8 texture upload/draw path
- [x] Native direct-light PBR pass with normals, metalness, roughness, and emissive
- [x] Three.js matrixWorld transport for animated object transforms
- [x] Three.js Points to native billboard particle effect path
- [x] Native WebGPU navigator/device resource smoke path (buffer, texture, shader module)
- [x] WebGPU mapped buffer upload, native canvas/DOM shim, and WGSL compatibility normalization
- [x] Boa/Three.js node-cache compatibility shim for standard WebGPURenderer initialization
- [x] Native WebGPU bind group/pipeline/render-pass/compute-pass command execution slice
- [x] Native pipeline-derived bind group layouts and GPU buffer/texture copy commands
- [x] WebGPU texture arrays, mip/sample descriptors, view descriptors, and typed upload layout
- [x] Initial GPUCanvasContext configure/unconfigure and native swapchain presentation path
- [x] Project-relative fetch/Request/Response/Headers and ArrayBuffer/TextDecoder asset boundary
- [x] Three.js 0.179 WebGPURenderer PBR/DirectionalLight/Points scene smoke on native Metal
- [x] Three.js 0.179 AnimationMixer transform and morph-target shader smoke on native Metal
- [x] Three.js 0.179 SkinnedMesh/Skeleton and bone-transform AnimationMixer smoke on native Metal
- [x] Three.js 0.179 MeshStandardNodeMaterial, TSL colorNode, PostProcessing, pass, and Bloom smoke on native Metal
- [x] Three.js 0.179 GLTFLoader embedded-buffer SkinnedMesh/Skeleton/AnimationMixer/WebGPU fixture
- [x] Three.js 0.179 GLTFLoader GLB, external-buffer, external-PNG texture, and canvas-resize fixture
- [x] Three.js 0.179 standard WebGPURenderer InstancedMesh, BatchedMesh, Line, Sprite, Shadow, Environment, MRT, and indirect-draw/readback fixture
- [x] WebGPU `GPUBuffer` copy/submit/`mapAsync(READ)` readback fixture
- [x] WebGPU GPUQuerySet occlusion queries, pass begin/end, resolveQuerySet, and readback fixture
- [x] WebGPU texture/sampler destroy, native error-scope, and device-lost lifecycle bindings
- [x] Device-loss stale-surface guard and native Renderer/JS-session restart smoke
- [x] WebGPU canvas configure/unconfigure, surface texture lifetime cleanup, and Lost/Outdated native surface reconfiguration
- [x] Three.js texture-array `textureLoad` LOD WGSL normalization for current Naga
- [x] Native AssetStore `EXT_meshopt_compression` attribute/triangle/index-sequence decode with filters
- [ ] Complete canvas resize/device-loss/present lifecycle and transparent alpha fidelity for Three.js WebGPURenderer
- [x] Three.js WebGPU compressed texture format negotiation and mip-level uploads
- [ ] Three.js WebGPU renderer bindings for textures, materials, and command objects
- [x] Three.js GLTFLoader MeshoptDecoder injection and compressed asset end-to-end fixture
- [x] Standard KTX2Loader raw BC1 path through KHR_texture_basisu and native GPU upload
- [x] Native BasisLZ/UASTC transcoder binding with KTX2Loader bridge and compressed/uncompressed target selection
- [x] Native raw KTX2 mip/face transfer without a browser Worker
- [x] UASTC KTX2 end-to-end fixture for RGBA32 and BC7 target paths
- [ ] UASTC target matrix across ASTC/BC3/BC1/ETC2 on Windows/Linux GPU backends
- [x] DRACO native mesh decode through standard GLTFLoader/DRACOLoader with Khronos Box fixture
- [ ] DRACO attribute/point-cloud/standalone API coverage and full material/animation GPU upload
- [ ] GPU-driven culling, indirect draw, and large-scene benchmark suite
- [ ] Signed installers and release update channels for all three platforms

## Commerce

- [ ] Creator identity and Connected Account onboarding
- [ ] Catalog, immutable game releases, and platform compatibility metadata
- [ ] Checkout, entitlements, refunds, disputes, and idempotent webhooks
- [ ] Versioned platform fee schedule and settlement ledger
- [ ] Creator earnings/payout dashboard
- [ ] Platform admin/reconciliation dashboard
- [ ] Express Dashboard MVP, followed by embedded Connect components
- [ ] Tax, KYC/AML, merchant-of-record, negative-balance, and refund policy review
- [ ] Test-mode pilot, restricted live pilot, and staged public launch

The cross-platform runtime plan is in
[`docs/platform-support.md`](platform-support.md). The commerce design is in
[`docs/commerce-connect-plan.md`](commerce-connect-plan.md).
