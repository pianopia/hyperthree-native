# Three.js GLTFLoader native smoke

This fixture verifies the standard Three.js loading path rather than the
native `drawAsset()` shortcut. It loads a project-relative glTF with an
embedded buffer, an external-buffer glTF with an external PNG texture, a GLB
with an embedded PNG texture, an `EXT_meshopt_compression` glTF, and a
`KHR_texture_basisu` glTFs using raw BC1 and BasisLZ KTX2 textures. It creates
`SkinnedMesh`/`Skeleton` objects,
advances an `AnimationMixer`, renders `InstancedMesh`/`Line`/`Sprite` objects,
`BatchedMesh`, shadows, an equirectangular environment, an MRT target, and an
indirect draw into an offscreen target whose pixels are read back with
`WebGPURenderer`.

From the repository root:

```bash
npm install --prefix tests/fixtures/threejs-gltf-loader-smoke
npm run build --prefix tests/fixtures/threejs-gltf-loader-smoke
cargo run -- run --project tests/fixtures/threejs-gltf-loader-smoke --skip-build
```

The native host must have a visible GPU backend. The fixture generates its
binary assets during `npm run build` and fails during startup if GLTFLoader,
KTX2Loader, external-buffer/image loading, skin creation, animation setup, GLB
parsing, compressed texture upload, or the WebGPU render does not settle. The
raw BC1 asset exercises the native KTX2 hook's raw-format fallback, and the
BasisLZ asset exercises native transcoding with the current device target.
Additional UASTC target-format fixtures are tracked separately.
