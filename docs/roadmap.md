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
- [ ] Native audio and sandboxed filesystem data APIs
- [ ] Embedded V8 isolate and module/runtime compatibility
- [x] Three.js scene sync for BoxGeometry, PlaneGeometry, and SphereGeometry
- [x] Three.js position/index BufferGeometry registration and native GPU draw path
- [x] BufferGeometry UV attributes and glTF base-color RGBA8 texture upload/draw path
- [ ] Three.js WebGPU renderer bindings for textures, materials, and command objects
- [ ] glTF/KTX2 zero-copy texture decode and full material/animation GPU upload
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
