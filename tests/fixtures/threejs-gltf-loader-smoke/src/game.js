import * as THREE from "three";
import { WebGPURenderer } from "three/webgpu";
import { DRACOLoader } from "three/addons/loaders/DRACOLoader.js";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { KTX2Loader } from "three/addons/loaders/KTX2Loader.js";
import { AudioLoader } from "three";

const scene = new THREE.Scene();
const environmentTexture = new THREE.DataTexture(
  new Uint8Array([
    96, 128, 192, 255, 96, 128, 192, 255, 96, 128, 192, 255, 96, 128, 192, 255,
    96, 128, 192, 255, 96, 128, 192, 255, 96, 128, 192, 255, 96, 128, 192, 255,
  ]),
  4,
  2,
  THREE.RGBAFormat,
);
environmentTexture.colorSpace = THREE.SRGBColorSpace;
environmentTexture.mapping = THREE.EquirectangularReflectionMapping;
environmentTexture.needsUpdate = true;
scene.environment = environmentTexture;
const directionalLight = new THREE.DirectionalLight(0xffffff, 2.5);
directionalLight.position.set(2, 4, 3);
directionalLight.castShadow = true;
directionalLight.shadow.mapSize.set(256, 256);
directionalLight.shadow.camera.near = 0.1;
directionalLight.shadow.camera.far = 20;
scene.add(directionalLight);
const camera = new THREE.PerspectiveCamera(60, 16 / 9, 0.1, 100);
camera.position.z = 4;
camera.lookAt(0, 0, 0);

const featureGroup = new THREE.Group();
const instanced = new THREE.InstancedMesh(
  new THREE.BoxGeometry(0.35, 0.35, 0.35),
  new THREE.MeshStandardMaterial({ color: 0xff8844, roughness: 0.6, metalness: 0.1 }),
  2,
);
const instanceMatrix = new THREE.Matrix4();
instanceMatrix.makeTranslation(-1.2, -0.4, 0);
instanced.setMatrixAt(0, instanceMatrix);
instanceMatrix.makeTranslation(1.2, -0.4, 0);
instanced.setMatrixAt(1, instanceMatrix);
instanced.instanceMatrix.needsUpdate = true;
const line = new THREE.Line(
  new THREE.BufferGeometry().setFromPoints([
    new THREE.Vector3(-1, -1, 0),
    new THREE.Vector3(0, -0.4, 0),
    new THREE.Vector3(1, -1, 0),
  ]),
  new THREE.LineBasicMaterial({ color: 0x44ccff }),
);
const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ color: 0x66ff88 }));
sprite.position.set(0, 1.4, 0);
sprite.scale.setScalar(0.35);
const batched = new THREE.BatchedMesh(
  2,
  128,
  256,
  new THREE.MeshStandardMaterial({ color: 0xaa66ff, roughness: 0.7 }),
);
const batchedGeometryId = batched.addGeometry(new THREE.BoxGeometry(0.3, 0.3, 0.3));
const batchedInstanceId = batched.addInstance(batchedGeometryId);
instanceMatrix.makeTranslation(0, -1.2, 0);
batched.setMatrixAt(batchedInstanceId, instanceMatrix);
featureGroup.add(instanced, line, sprite, batched);
featureGroup.traverse((object) => {
  if (object.isMesh || object.isBatchedMesh || object.isInstancedMesh) {
    object.castShadow = true;
    object.receiveShadow = true;
  }
});
scene.add(featureGroup);

globalThis.__gltfSmokeError = null;
globalThis.__gltfSmokeLoaded = false;
globalThis.__gltfSmokeRendered = false;
globalThis.__gltfExternalTexture = false;
globalThis.__gltfGlbLoaded = false;
globalThis.__gltfMeshoptLoaded = false;
globalThis.__gltfKtx2Loaded = false;
globalThis.__gltfKtx2NativeHook = false;
globalThis.__gltfUastcKtx2Loaded = false;
globalThis.__gltfAudioLoaded = false;
globalThis.__gltfAudioFilter = false;
globalThis.__gltfAudioAnalyser = false;
globalThis.__gltfPositionalAudio = false;
globalThis.__gltfBasisKtx2Loaded = false;
globalThis.__gltfDracoLoaded = false;
globalThis.__gltfFeatureSmoke = false;
globalThis.__gltfBatchedSmoke = false;
globalThis.__gltfShadowSmoke = false;
globalThis.__gltfEnvironmentSmoke = false;
globalThis.__gltfMrtSmoke = false;
globalThis.__gltfIndirectSmoke = false;
globalThis.__gltfReadbackSmoke = false;
globalThis.__gltfMappedBufferSmoke = false;
globalThis.__gltfQuerySmoke = false;
globalThis.__gltfResourceLifecycleSmoke = false;
globalThis.__gltfCanvasLifecycleSmoke = false;
globalThis.__gltfResizeEvent = false;
globalThis.__gltfSmokeReady = false;
let smokeFrames = 0;
window.addEventListener("resize", () => { globalThis.__gltfResizeEvent = window.innerWidth === 960 && window.innerHeight === 540; });
globalThis.__gltfSmokeStage = "before-adapter";

navigator.gpu.requestAdapter().then(async (adapter) => {
  globalThis.__gltfSmokeStage = "after-adapter";
  const device = await adapter.requestDevice();
  const sourceBuffer = device.createBuffer({ size: 4, usage: GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST });
  const readbackBuffer = device.createBuffer({ size: 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  device.queue.writeBuffer(sourceBuffer, 0, new Uint8Array([7, 11, 13, 17]));
  const readbackEncoder = device.createCommandEncoder();
  readbackEncoder.copyBufferToBuffer(sourceBuffer, 0, readbackBuffer, 0, 4);
  device.queue.submit([readbackEncoder.finish()]);
  await readbackBuffer.mapAsync(GPUMapMode.READ);
  const readbackBytes = new Uint8Array(readbackBuffer.getMappedRange());
  globalThis.__gltfReadbackSmoke = readbackBytes[0] === 7 && readbackBytes[1] === 11 && readbackBytes[2] === 13 && readbackBytes[3] === 17;
  readbackBuffer.unmap();
  const mappedUpload = device.createBuffer({ size: 4, usage: GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST, mappedAtCreation: true });
  new Uint8Array(mappedUpload.getMappedRange()).set([19, 23, 29, 31]);
  mappedUpload.unmap();
  const mappedReadback = device.createBuffer({ size: 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  const mappedEncoder = device.createCommandEncoder();
  mappedEncoder.copyBufferToBuffer(mappedUpload, 0, mappedReadback, 0, 4);
  device.queue.submit([mappedEncoder.finish()]);
  await mappedReadback.mapAsync(GPUMapMode.READ);
  const mappedBytes = new Uint8Array(mappedReadback.getMappedRange());
  globalThis.__gltfMappedBufferSmoke = mappedBytes[0] === 19 && mappedBytes[1] === 23 && mappedBytes[2] === 29 && mappedBytes[3] === 31;
  mappedReadback.unmap();
  mappedUpload.destroy();
  mappedReadback.destroy();
  sourceBuffer.destroy();
  readbackBuffer.destroy();
  const indirectTarget = device.createTexture({
    size: { width: 4, height: 4 },
    format: "rgba8unorm",
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
  });
  const indirectReadback = device.createBuffer({ size: 1024, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  const occlusionQuerySet = device.createQuerySet({ type: "occlusion", count: 1 });
  const occlusionResolve = device.createBuffer({ size: 8, usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC });
  const occlusionReadback = device.createBuffer({ size: 8, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  const indirectArgs = device.createBuffer({ size: 16, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.INDIRECT });
  device.queue.writeBuffer(indirectArgs, 0, new Uint32Array([3, 1, 0, 0]));
  const indirectShader = device.createShaderModule({ code: `
    @vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
      var positions = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
      return vec4f(positions[index], 0.0, 1.0);
    }
    @fragment fn fs_main() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }
  ` });
  const indirectPipeline = device.createRenderPipeline({
    layout: "auto",
    vertex: { module: indirectShader, entryPoint: "vs_main", buffers: [] },
    fragment: { module: indirectShader, entryPoint: "fs_main", targets: [{ format: "rgba8unorm" }] },
    primitive: { topology: "triangle-list" },
  });
  const indirectEncoder = device.createCommandEncoder();
  const indirectPass = indirectEncoder.beginRenderPass({ colorAttachments: [{
    view: indirectTarget.createView(),
    loadOp: "clear",
    storeOp: "store",
    clearValue: { r: 0, g: 0, b: 0, a: 1 },
  }], occlusionQuerySet });
  indirectPass.setPipeline(indirectPipeline);
  indirectPass.beginOcclusionQuery(0);
  indirectPass.drawIndirect(indirectArgs, 0);
  indirectPass.endOcclusionQuery();
  indirectPass.end();
  indirectEncoder.copyTextureToBuffer(
    { texture: indirectTarget },
    { buffer: indirectReadback, bytesPerRow: 256, rowsPerImage: 4 },
    { width: 4, height: 4, depthOrArrayLayers: 1 },
  );
  indirectEncoder.resolveQuerySet(occlusionQuerySet, 0, 1, occlusionResolve, 0);
  indirectEncoder.copyBufferToBuffer(occlusionResolve, 0, occlusionReadback, 0, 8);
  device.queue.submit([indirectEncoder.finish()]);
  await indirectReadback.mapAsync(GPUMapMode.READ);
  const indirectBytes = new Uint8Array(indirectReadback.getMappedRange());
  globalThis.__gltfIndirectSmoke = indirectBytes[0] === 255 && indirectBytes[1] === 0 && indirectBytes[2] === 0 && indirectBytes[3] === 255;
  indirectReadback.unmap();
  await occlusionReadback.mapAsync(GPUMapMode.READ);
  const occlusionResult = new BigUint64Array(occlusionReadback.getMappedRange());
  globalThis.__gltfQuerySmoke = occlusionResult[0] > 0n;
  occlusionReadback.unmap();
  occlusionQuerySet.destroy();
  occlusionResolve.destroy();
  occlusionReadback.destroy();
  indirectArgs.destroy();
  indirectReadback.destroy();
  indirectTarget.destroy();
  device.pushErrorScope("validation");
  const errorScope = await device.popErrorScope();
  const temporaryTexture = device.createTexture({ size: { width: 1, height: 1 }, format: "rgba8unorm", usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
  temporaryTexture.createView();
  temporaryTexture.destroy();
  const temporarySampler = device.createSampler();
  temporarySampler.destroy();
  globalThis.__gltfResourceLifecycleSmoke = errorScope === null;
  const renderer = new WebGPURenderer({ canvas: globalThis.__hyperthreeNativeCanvas, antialias: false });
  globalThis.__gltfSmokeStage = "before-renderer-init";
  await renderer.init();
  const ktx2Loader = new KTX2Loader();
  ktx2Loader.detectSupport(renderer);
  renderer.setSize(640, 360, false);
  renderer.setSize(960, 540, false);
  globalThis.__gltfSmokeStage = "before-gltf-load";
  const loader = new GLTFLoader();
  const audioLoader = new AudioLoader();
  const audioLoad = audioLoader.loadAsync("public/generated/tone.wav")
    .then((audioBuffer) => {
      const listener = new THREE.AudioListener();
      const sound = new THREE.Audio(listener);
      sound.setBuffer(audioBuffer);
      const filter = sound.context.createBiquadFilter();
      filter.type = 'lowpass';
      filter.frequency.setValueAtTime(900, sound.context.currentTime);
      filter.Q.setValueAtTime(0.7, sound.context.currentTime);
      sound.setFilters([filter]);
      globalThis.__gltfAudioFilter = sound.getFilter() === filter &&
        filter.type === 'lowpass' && filter.frequency.value === 900 && filter.Q.value === 0.7;
      const analyser = new THREE.AudioAnalyser(sound, 32);
      const frequencyData = analyser.getFrequencyData();
      globalThis.__gltfAudioAnalyser = frequencyData.length === 16 &&
        analyser.getAverageFrequency() >= 0;
      const positional = new THREE.PositionalAudio(listener);
      positional.setBuffer(audioBuffer);
      positional.setRefDistance(2);
      positional.setRolloffFactor(0.75);
      positional.panner.positionX.setValueAtTime(1.5, positional.context.currentTime);
      globalThis.__gltfPositionalAudio = positional.panner.refDistance === 2 &&
        positional.panner.rolloffFactor === 0.75 && positional.panner.positionX.value === 1.5;
      return audioBuffer;
    });
  const dracoLoader = new DRACOLoader();
  loader.setDRACOLoader(dracoLoader);
  loader.setKTX2Loader(ktx2Loader);
  const trackLoad = (label, promise) => promise.then((value) => {
    globalThis.__gltfSmokeStage = `loaded-${label}`;
    return value;
  });
  const loadedAssets = await Promise.all([
    trackLoad("gltf", loader.loadAsync("public/scene.gltf")),
    trackLoad("external", loader.loadAsync("public/generated/scene-external.gltf")),
    trackLoad("glb", loader.loadAsync("public/generated/scene.glb")),
    trackLoad("meshopt", loader.loadAsync("public/generated/scene-meshopt.gltf")),
    trackLoad("ktx2", loader.loadAsync("public/generated/scene-ktx2.gltf")),
    trackLoad("basis", loader.loadAsync("public/generated/scene-ktx2-basis.gltf")),
    trackLoad("uastc", loader.loadAsync("public/generated/scene-ktx2-uastc.gltf")),
    trackLoad("draco", loader.loadAsync("public/generated/scene-draco.gltf")),
    trackLoad("audio", audioLoad),
  ]);
  const gltf = loadedAssets[0];
  const externalGltf = loadedAssets[1];
  const glb = loadedAssets[2];
  const meshoptGltf = loadedAssets[3];
  const ktx2Gltf = loadedAssets[4];
  const basisKtx2Gltf = loadedAssets[5];
  const uastcKtx2Gltf = loadedAssets[6];
  const dracoGltf = loadedAssets[7];
  const audioBuffer = loadedAssets[8];
  globalThis.__gltfSmokeStage = "after-gltf-load";
  const skinned = gltf.scene.getObjectByProperty("isSkinnedMesh", true);
  if (!skinned || !skinned.skeleton || skinned.skeleton.bones.length !== 2) {
    throw new Error("GLTFLoader did not create the expected SkinnedMesh/Skeleton");
  }
  const mixer = new THREE.AnimationMixer(gltf.scene);
  mixer.clipAction(gltf.animations[0]).play();
  mixer.update(0.25);
  globalThis.__gltfSmokeLoaded = gltf.scene.children.length === 1 && gltf.animations.length === 1;
  const textured = externalGltf.scene.getObjectByProperty("isMesh", true);
  globalThis.__gltfExternalTexture = Boolean(
    textured?.material?.map?.image?.width === 1 &&
    textured.material.map.image.height === 1 &&
    textured.material.map.image.data?.byteLength === 4,
  );
  globalThis.__gltfGlbLoaded = glb.scene.children.length === 1 && glb.animations.length === 1;
  const meshoptMesh = meshoptGltf.scene.getObjectByProperty("isMesh", true);
  globalThis.__gltfMeshoptLoaded = Boolean(
    meshoptMesh?.geometry?.attributes?.position?.count === 3 &&
    meshoptMesh.geometry.index?.count === 3,
  );
  const ktx2Textured = ktx2Gltf.scene.getObjectByProperty("isMesh", true);
  globalThis.__gltfKtx2Loaded = Boolean(
    ktx2Textured?.material?.map?.isCompressedTexture === true &&
    ktx2Textured.material.map.image?.width === 4 &&
    ktx2Textured.material.map.image?.height === 4,
  );
  globalThis.__gltfKtx2NativeHook = globalThis.__hyperthreeKtx2NativeCalls > 0;
  const basisKtx2Textured = basisKtx2Gltf.scene.getObjectByProperty("isMesh", true);
  globalThis.__gltfBasisKtx2Loaded = Boolean(
    basisKtx2Textured?.material?.map?.isCompressedTexture === true &&
    basisKtx2Textured.material.map.image?.width > 0 &&
    basisKtx2Textured.material.map.image?.height > 0,
  );
  const uastcKtx2Textured = uastcKtx2Gltf.scene.getObjectByProperty("isMesh", true);
  globalThis.__gltfUastcKtx2Loaded = Boolean(
    uastcKtx2Textured?.material?.map?.image?.width === 4 &&
    uastcKtx2Textured.material.map.image.height === 4 &&
    uastcKtx2Textured.material.map.image.data?.byteLength === 64,
  );
  globalThis.__gltfAudioLoaded = Boolean(
    audioBuffer?.sampleRate === 8000 &&
    audioBuffer.length === 4 &&
    audioBuffer.numberOfChannels === 1 &&
    Math.abs(audioBuffer.getChannelData(0)[1] - 0.25) < 0.01,
  );
  const dracoMesh = dracoGltf.scene.getObjectByProperty("isMesh", true);
  globalThis.__gltfDracoLoaded = Boolean(
    dracoMesh?.geometry?.attributes?.position?.count === 24 &&
    dracoMesh.geometry.attributes.normal?.count === 24 &&
    dracoMesh.geometry.index?.count === 36 &&
    globalThis.__hyperthreeDracoNativeCalls > 0,
  );
  if (!globalThis.__gltfExternalTexture) throw new Error("external glTF image texture did not decode");
  if (!globalThis.__gltfGlbLoaded) throw new Error("GLB container did not load through GLTFLoader");
  scene.add(gltf.scene);
  scene.add(externalGltf.scene);
  scene.add(glb.scene);
  scene.add(ktx2Gltf.scene);
  scene.add(basisKtx2Gltf.scene);
  scene.add(uastcKtx2Gltf.scene);
  scene.add(dracoGltf.scene);
  globalThis.__gltfEnvironmentSmoke = scene.environment === environmentTexture;
  globalThis.__gltfSmokeStage = "before-render";
  const mrt = new THREE.RenderTarget(64, 64, { count: 2, depthBuffer: true });
  renderer.setRenderTarget(mrt);
  await renderer.renderAsync(scene, camera);
  renderer.setRenderTarget(null);
  globalThis.__gltfMrtSmoke = mrt.isRenderTarget === true && mrt.texture.length === 2;
  await renderer.renderAsync(scene, camera);
  globalThis.__gltfSmokeRendered = device !== null && renderer.isWebGPURenderer === true;
  const canvasContext = globalThis.__hyperthreeNativeCanvas.getContext('webgpu');
  const canvasConfiguration = canvasContext.configuration;
  canvasContext.unconfigure();
  const canvasUnconfigured = canvasContext.configuration === null;
  canvasContext.configure(canvasConfiguration);
  globalThis.__gltfCanvasLifecycleSmoke = canvasUnconfigured && canvasContext.configuration === canvasConfiguration;
  globalThis.__gltfFeatureSmoke = instanced.isInstancedMesh && line.isLine && sprite.isSprite;
  globalThis.__gltfBatchedSmoke = batched.isBatchedMesh === true;
  globalThis.__gltfShadowSmoke = directionalLight.castShadow === true && directionalLight.shadow.mapSize.x === 256;
  globalThis.__gltfSmokeReady = true;
  if (!globalThis.__gltfSmokeLoaded || !globalThis.__gltfExternalTexture || !globalThis.__gltfGlbLoaded || !globalThis.__gltfMeshoptLoaded || !globalThis.__gltfKtx2Loaded || !globalThis.__gltfKtx2NativeHook || !globalThis.__gltfBasisKtx2Loaded || !globalThis.__gltfUastcKtx2Loaded || !globalThis.__gltfDracoLoaded || !globalThis.__gltfAudioLoaded || !globalThis.__gltfAudioFilter || !globalThis.__gltfAudioAnalyser || !globalThis.__gltfPositionalAudio ||
      !globalThis.__gltfResizeEvent || !globalThis.__gltfFeatureSmoke || !globalThis.__gltfBatchedSmoke ||
      !globalThis.__gltfShadowSmoke || !globalThis.__gltfEnvironmentSmoke || !globalThis.__gltfMrtSmoke ||
      !globalThis.__gltfIndirectSmoke || !globalThis.__gltfReadbackSmoke || !globalThis.__gltfMappedBufferSmoke || !globalThis.__gltfQuerySmoke || !globalThis.__gltfResourceLifecycleSmoke ||
      !globalThis.__gltfCanvasLifecycleSmoke) {
    throw new Error("standard Three.js compatibility fixture assertions failed");
  }
}).catch((error) => {
  globalThis.__gltfSmokeError = `${globalThis.__gltfSmokeStage}: ${String(error.stack || error)}`;
});

globalThis.HyperThreeGame = {
  update() {
    if (globalThis.__gltfSmokeError) throw new Error(globalThis.__gltfSmokeError.replace(/\n/g, " | "));
    smokeFrames += 1;
    if (smokeFrames > 180 && !globalThis.__gltfSmokeReady) throw new Error("standard Three.js compatibility fixture timed out");
  },
  onStart() {
    if (globalThis.__gltfSmokeError) throw new Error(globalThis.__gltfSmokeError.replace(/\n/g, " | "));
  },
  onStop() {},
};
