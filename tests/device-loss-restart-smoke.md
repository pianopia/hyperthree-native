# Device-loss restart smoke

This direct JavaScript fixture destroys the first native `GPUDevice`. The host
must skip stale surface work, recreate the native Metal device/Renderer/runtime,
and execute the same entry point once more.

From the repository root:

```sh
cargo build
RUST_LOG=info ./target/debug/hyperthree-native --script tests/device-loss-restart-smoke.js
```

The expected log contains `native GPU device lost; restarting game session` and
`restarted native game session after device loss`. The process is intended to
remain running after the successful restart; use the normal window close action
to stop it.
