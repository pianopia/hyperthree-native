# HyperThree Native - implementation map

This repository is the first executable vertical slice of the supplied
architecture specification.

| Specification concern | Current implementation | Next increment |
| --- | --- | --- |
| Native host / direct swapchain | `src/renderer.rs` creates a native `winit` window and a `wgpu` surface | platform-specific Vulkan / Metal / DirectX validation |
| JS execution outside a browser | `src/js_runtime.rs` evaluates the entry script in an embeddable runtime | replace the adapter with Embedded V8 and expose the isolate lifecycle |
| Zero-copy asset path | `src/asset.rs` maps binary files with `memmap2` | native glTF / KTX2 decoder and direct GPU upload |
| Three.js compatibility seam | `js/three-bridge.js` defines the browser-free native contract | bind WebGPU objects and run the Three.js WebGPU backend |
| GPU-driven rendering | renderer owns the native render pass | compute culling, indirect draw buffers, and instancing benchmark |

The project deliberately keeps the JS, asset, and graphics layers independent.
That makes the expensive V8 and native decoder work incremental instead of
coupling it to window bring-up.

