# HyperThree Native - implementation map

This repository is the first executable vertical slice of the supplied
architecture specification.

| Specification concern | Current implementation | Next increment |
| --- | --- | --- |
| Native host / direct swapchain | `src/renderer.rs` creates a native `winit` window and a `wgpu` surface | platform-specific Vulkan / Metal / DirectX validation |
| JS execution outside a browser | `src/js_runtime.rs` runs embedded Boa scripts and ES modules with project-relative/`node_modules`/`exports` resolution, native `performance.now()`, RAF scheduling, window-global aliases, project-relative/data/blob URL `fetch`, `Request`/`Response`/`Blob`/`File`/`URL.createObjectURL`, `ArrayBuffer`/`TextDecoder`/`createImageBitmap`, a native Web Audio decode/playback bridge (`AudioContext`, `AudioBuffer`, `AudioBufferSourceNode`, `GainNode`, and `PannerNode` shape), and an opt-in `navigator.gpu` resource binding; the WebGPU shim includes mapped buffer upload, native canvas/DOM types, WGSL diagnostic compatibility, Three.js node-cache compatibility, navigator metadata, console compatibility, device-lost Promise delivery, native error scopes, compressed texture feature negotiation, and mip-level-aware texture uploads | Embedded V8 isolate, broader Node/Web API compatibility, filesystem sandbox, and full Web Audio graph/spatial DSP |
| Asset path | `src/asset.rs` memory-maps project-relative files, inspects glTF/GLB metadata, decodes POSITION/index/UV primitives, EXT_meshopt-compressed views, and base-color images natively; the standard GLTFLoader path verifies native raw BC1 KTX2 level transfer, BasisLZ/UASTC (RGBA32/BC7 target) transcoding, and Khronos Box `KHR_draco_mesh_compression` decode through DRACOLoader | ASTC/BC3/BC1/ETC2 target matrix fixtures, Draco attribute/point-cloud/standalone coverage, and full material/animation streaming |
| JS-to-native render bridge | `src/bridge.rs` and `js/three-bridge.js` now carry position/normal/UV geometry, PBR material parameters, directional light, `matrixWorld`, and `Points` particles into native PBR/billboard passes; `src/webgpu.rs` creates native resources, derives pipeline bind-group layouts, executes WebGPU bind-group/pipeline/render/compute/copy commands, supports texture view descriptors, typed and compressed texture uploads across mip levels, GPUBuffer MAP_READ readback, texture/sampler destruction, device-lost signaling, and presents a canvas texture through the native surface; the host recreates the device/Renderer/JS session at the next frame boundary after loss | Complete renderer bindings and device-loss recovery |
| Three.js compatibility seam | `syncThreeScene(scene, camera)` remains a migration bridge, while standard Three.js 0.179 `WebGPURenderer.renderAsync()` has been smoke-tested with PBR/light/Points, NodeMaterial/TSL/PostProcessing/Bloom, AnimationMixer transform/morph paths, GLTFLoader embedded/external/GLB assets with PNG textures, InstancedMesh/BatchedMesh/Line/Sprite, shadows, environment, MRT, indirect draw with GPU readback, and canvas resize propagation on native Metal; device-lost and native error-scope bindings are also connected | complete WebGPU renderer/object/material/asset coverage |
| GPU-driven rendering | renderer owns the native render pass | compute culling, indirect draw buffers, and instancing benchmark |
| Distribution and monetization | roadmap and commerce design are documented separately | cross-platform packaging, Connect onboarding, checkout, ledger, payouts, and dashboards |

The project deliberately keeps the JS, asset, and graphics layers independent.
That makes the expensive V8 and native decoder work incremental instead of
coupling it to window bring-up.

The current bridge is intentionally a migration layer, not the final compatibility
boundary. A game can call
`HyperThreeNative.setClearColor()`, `setCamera()`, `beginFrame()`, and
`pushCube()`; the native host invokes `HyperThreeGame.update(deltaSeconds)` on
each frame and consumes the resulting instance list. `isKeyDown()` exposes
physical keyboard state, while optional `onStart()` and `onStop()` callbacks
cover host lifecycle edges. `syncThreeScene(scene, camera)` provides the first
scene-derived compatibility path for box, plane, sphere, and arbitrary
position/index BufferGeometry primitives. `loadAsset(path)` retains the mapped
asset in the native store while returning format and glTF metadata, while
`drawAsset(path, meshIndex, primitiveIndex, options)` decodes a glTF/GLB
primitive natively and uploads it to the cached native geometry, PBR material,
and base-color texture paths. The initial standard WebGPU resource and command
binding is now available for offscreen native GPU work. The next architectural
step is the complete canvas lifecycle and broader standard WebGPU object
binding described in
[`docs/threejs-compatibility-architecture.md`](threejs-compatibility-architecture.md);
the current bridge is not yet a complete Three.js WebGPURenderer integration,
skinning/animation runtime, or effects pipeline.
