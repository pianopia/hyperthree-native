# Three.js GLTFLoader native smoke

This fixture verifies the standard Three.js loading path rather than the
native `drawAsset()` shortcut. It loads a project-relative glTF with an
embedded buffer, an external-buffer glTF with an external PNG texture, and a
GLB with an embedded PNG texture. It creates `SkinnedMesh`/`Skeleton` objects,
advances an `AnimationMixer`, renders `InstancedMesh`/`Line`/`Sprite` objects,
`BatchedMesh`, shadows, an equirectangular environment, and an MRT target, and
exercises GPU buffer readback with `WebGPURenderer`.

From the repository root:

```bash
npm install --prefix tests/fixtures/threejs-gltf-loader-smoke
npm run build --prefix tests/fixtures/threejs-gltf-loader-smoke
cargo run -- run --project tests/fixtures/threejs-gltf-loader-smoke --skip-build
```

The native host must have a visible GPU backend. The fixture generates its
binary assets during `npm run build` and fails during startup if GLTFLoader,
external-buffer/image loading, skin creation, animation setup, GLB parsing, or
the WebGPU render does not settle.
