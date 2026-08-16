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
animation state, but do not use document, WebGLRenderer, or browser-only input
APIs until the standard WebGPU binding phase is enabled.
Expose the playable state through globalThis.HyperThreeGame.
Use HyperThreeNative.setClearColor(), setCamera(), beginFrame(), and
syncThreeScene() (or pushCube(), pushPlane(), and pushSphere()) for the current
native rendering bridge. Put per-frame logic in
HyperThreeGame.update(deltaSeconds), and use HyperThreeNative.isKeyDown("KeyW")
for keyboard input. When using a Three.js Scene, call
HyperThreeNative.syncThreeScene(scene, camera) each frame; its current native
geometry coverage includes BoxGeometry, PlaneGeometry, SphereGeometry, and
position/normal/index/UV BufferGeometry. Standard material color, metalness,
roughness, emissive, DirectionalLight, and matrixWorld transforms are transported
to the native direct-light PBR pass. Use HyperThreeNative.loadAsset("public/models/example.glb")
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

This avoids making the native host understand every web bundler. The next SDK
increment will provide native replacements for the browser APIs commonly used
by Three.js renderers.
