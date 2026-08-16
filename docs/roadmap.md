# HyperThree Native roadmap

## Runtime

- [x] Native Rust host and wgpu surface prototype
- [x] AI-friendly project manifest and Vite bundle import path
- [x] JavaScript-to-native camera/mesh/material bridge
- [ ] macOS Metal, Windows DirectX 12/Vulkan, Linux Vulkan/software CI matrix
- [x] Cross-platform CI skeleton and headless GPU backend diagnostics
- [x] Native keyboard input, frame clock, and start/update/stop lifecycle callbacks
- [ ] Native audio and sandboxed filesystem APIs
- [ ] Embedded V8 isolate and module/runtime compatibility
- [ ] Three.js WebGPU renderer bindings
- [ ] glTF/KTX2 zero-copy decode and GPU upload
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
