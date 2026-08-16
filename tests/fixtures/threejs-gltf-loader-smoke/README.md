# Three.js GLTFLoader native smoke

This fixture verifies the standard Three.js loading path rather than the
native `drawAsset()` shortcut. It loads a project-relative glTF with an
embedded buffer, creates a `SkinnedMesh` and `Skeleton`, advances an
`AnimationMixer`, and renders the result with `WebGPURenderer`.

From the repository root:

```bash
npm install --prefix tests/fixtures/threejs-gltf-loader-smoke
npm run build --prefix tests/fixtures/threejs-gltf-loader-smoke
cargo run -- run --project tests/fixtures/threejs-gltf-loader-smoke --skip-build
```

The native host must have a visible GPU backend. The fixture fails during
startup if loading, skin creation, animation setup, or the WebGPU render does
not settle.
