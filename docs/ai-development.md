# AI-first development workflow

## Contract

The AI agent edits `src/game.js` and keeps the game entry browser-independent.
The native host reads `hyperthree.toml`; this file is the boundary between the
generated project and the runtime.

## Prompt template

```text
You are building a Three.js game for HyperThree Native.
Edit only src/game.js and public/ assets.
Use Three.js scene, camera, geometry, standard/physical materials, lights, and
animation state. New games should use the standard `WebGPURenderer` with
`HyperThreeNative.canvas`; the generated project already provides this setup,
including PBR, resize handling, and the native-hosted frame loop.
Expose the playable state through globalThis.HyperThreeGame.
The older `HyperThreeNative.setClearColor()`/`syncThreeScene()` (or
`pushCube()`, `pushPlane()`, and `pushSphere()`) path remains available for
migration and diagnostics, but is not the default for new games. Put per-frame logic in
HyperThreeGame.update(deltaSeconds), and use HyperThreeNative.isKeyDown("KeyW")
for keyboard input. Use `HyperThreeNative.loadAsset("public/models/example.glb")`
for native asset metadata and HyperThreeNative.drawAsset("public/models/example.glb",
0, 0, options) for native glTF primitive rendering. Optional onStart() and
onStop() callbacks are available. Use isMouseButtonDown(0) and
getMousePosition() for native pointer input.
Keep the project buildable with npm run build. The compatibility roadmap for
WebGPURenderer, skinning, post-processing, and effects is in
docs/threejs-compatibility-architecture.md.
```

## Importing an existing project

1. Add a `hyperthree.toml` at the project root.
2. Point `project.entry` at the JavaScript entry and `project.output` at a single
   IIFE bundle.
3. Configure the existing bundler to emit that IIFE bundle.
4. Run `hyperthree-native build --project <path>`.

This avoids making the native host understand every web bundler. The native
WebGPU binding already covers the standard renderer path used by the generated
project. Additional browser APIs should be added to the compatibility layer
when a game needs them, with an explicit capability check rather than a silent
rendering fallback.
