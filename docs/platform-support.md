# Cross-platform runtime plan

HyperThree Native is a Rust runtime, so the game-facing API must not expose
Metal, Direct3D, Vulkan, Wayland, or X11 directly. Windowing and GPU backend
selection stay behind the host layer.

## Target matrix

| Platform | Windowing | Primary GPU path | Distribution | Required validation |
| --- | --- | --- | --- | --- |
| macOS | winit | Metal | signed/notarized `.app` and `.dmg` | Apple Silicon first, Intel compatibility build, resize/fullscreen, entitlements |
| Windows | winit | DirectX 12, Vulkan fallback | signed `.exe`/MSIX | Windows 10/11, integrated GPU, discrete GPU, device loss, installer update |
| Linux | winit | Vulkan, software fallback | AppImage first; Flatpak/deb later | X11 and Wayland, common Mesa drivers, missing Vulkan, file permissions |

The exact backend is selected by wgpu at runtime. The game bundle only sees
the HyperThree bridge, not the operating system backend.

## Implementation phases

1. **Host abstraction**: move window title, size, input, clock, audio, and file
   paths behind platform-neutral Rust traits.
2. **Backend smoke tests**: run the same clear-color and camera/cube bridge
   test on macOS Metal, Windows DirectX 12/Vulkan, and Linux Vulkan/software.
3. **Input and lifecycle**: normalize keyboard, mouse, gamepad, focus,
   suspend/resume, resize, fullscreen, and device-lost events.
4. **Packaging**: add reproducible release artifacts, signing/notarization,
   crash logs, update channels, and per-platform asset directories.
5. **Performance gates**: measure startup time, memory, frame pacing, asset
   upload time, and 10k/100k/500k object scenes on each platform.

## CI matrix

GitHub Actions should run at least:

- `macos-latest`: compile and native smoke test on Metal.
- `windows-latest`: compile with DirectX 12 and Vulkan feature coverage.
- `ubuntu-latest`: compile on X11/Wayland dependencies and run Vulkan or
  software-renderer smoke tests.
- A separate packaging job: produce checksums and attach artifacts only from
  tagged releases.

The first workflow is `.github/workflows/ci.yml`. It runs formatting, tests,
Clippy, release compilation, and the headless `cargo run -- diagnostics` probe
on all three OS families.

GPU screenshot tests should be tolerant of vendor-specific pixels. The first
gate is successful frame submission plus a small set of semantic render-state
assertions; image snapshots come after the renderer is deterministic enough.
