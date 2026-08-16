# HyperThree Native - implementation map

This repository is the first executable vertical slice of the supplied
architecture specification.

| Specification concern | Current implementation | Next increment |
| --- | --- | --- |
| Native host / direct swapchain | `src/renderer.rs` creates a native `winit` window and a `wgpu` surface | platform-specific Vulkan / Metal / DirectX validation |
| JS execution outside a browser | `src/js_runtime.rs` runs Boa scripts and ES modules with project-relative and `node_modules` resolution | Embedded V8 isolate and broader Node/Web API compatibility |
| Zero-copy asset path | `src/asset.rs` maps project-relative files and inspects glTF/GLB mesh metadata | native glTF / KTX2 geometry decode and direct GPU upload |
| JS-to-native render bridge | `src/bridge.rs` and `js/three-bridge.js` pass clear color, camera, per-frame cube/plane/sphere instances, material color, keyboard state, and asset metadata into wgpu | WebGPU device, buffer, texture, and command encoder bindings |
| Three.js compatibility seam | `syncThreeScene(scene, camera)` traverses Three.js BoxGeometry, PlaneGeometry, and SphereGeometry meshes into the native render list | complete WebGPU renderer/object/material coverage |
| GPU-driven rendering | renderer owns the native render pass | compute culling, indirect draw buffers, and instancing benchmark |
| Distribution and monetization | roadmap and commerce design are documented separately | cross-platform packaging, Connect onboarding, checkout, ledger, payouts, and dashboards |

The project deliberately keeps the JS, asset, and graphics layers independent.
That makes the expensive V8 and native decoder work incremental instead of
coupling it to window bring-up.

The current bridge is intentionally small but live: a game can call
`HyperThreeNative.setClearColor()`, `setCamera()`, `beginFrame()`, and
`pushCube()`; the native host invokes `HyperThreeGame.update(deltaSeconds)` on
each frame and consumes the resulting instance list. `isKeyDown()` exposes
physical keyboard state, while optional `onStart()` and `onStop()` callbacks
cover host lifecycle edges. `syncThreeScene(scene, camera)` provides the first
scene-derived compatibility path for box, plane, and sphere primitives, and
`loadAsset(path)` retains the mapped asset in the native store while returning
format and glTF metadata. It is not yet a complete `GPUDevice` binding or a
general `BufferGeometry` uploader.
