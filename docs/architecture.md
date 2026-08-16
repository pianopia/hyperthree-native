# HyperThree Native - implementation map

This repository is the first executable vertical slice of the supplied
architecture specification.

| Specification concern | Current implementation | Next increment |
| --- | --- | --- |
| Native host / direct swapchain | `src/renderer.rs` creates a native `winit` window and a `wgpu` surface | platform-specific Vulkan / Metal / DirectX validation |
| JS execution outside a browser | `src/js_runtime.rs` runs embedded Boa scripts and ES modules with project-relative/`node_modules`/`exports` resolution, native `performance.now()`, RAF scheduling, and window-global aliases | Embedded V8 isolate and broader Node/Web API compatibility |
| Asset path | `src/asset.rs` memory-maps project-relative files, inspects glTF/GLB metadata, decodes POSITION/index/UV primitives and base-color images natively, and retains decoded geometry/textures for GPU upload | KTX2/Draco/Basis textures and full material/animation streaming |
| JS-to-native render bridge | `src/bridge.rs` and `js/three-bridge.js` now carry position/normal/UV geometry, PBR material parameters, directional light, and Three.js `matrixWorld` into a native direct-light PBR pass | Standard WebGPU device, resource, render/compute pass, and renderer bindings |
| Three.js compatibility seam | `syncThreeScene(scene, camera)` traverses built-in primitives and registers arbitrary Three.js BufferGeometry into the native render list | complete WebGPU renderer/object/material coverage |
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
and base-color texture paths. The next architectural step is the standard
WebGPU object binding described in
[`docs/threejs-compatibility-architecture.md`](threejs-compatibility-architecture.md);
the current bridge is not yet a complete `GPUDevice` binding, skinning/animation
runtime, or effects pipeline.
