import * as THREE from "three";
import { WebGPURenderer } from "three/webgpu";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";

const scene = new THREE.Scene();
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
featureGroup.add(instanced, line, sprite);
scene.add(featureGroup);

globalThis.__gltfSmokeError = null;
globalThis.__gltfSmokeLoaded = false;
globalThis.__gltfSmokeRendered = false;
globalThis.__gltfExternalTexture = false;
globalThis.__gltfGlbLoaded = false;
globalThis.__gltfFeatureSmoke = false;
globalThis.__gltfReadbackSmoke = false;
globalThis.__gltfResourceLifecycleSmoke = false;
globalThis.__gltfResizeEvent = false;
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
  sourceBuffer.destroy();
  readbackBuffer.destroy();
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
  renderer.setSize(640, 360, false);
  renderer.setSize(960, 540, false);
  globalThis.__gltfSmokeStage = "before-gltf-load";
  const loader = new GLTFLoader();
  const [gltf, externalGltf, glb] = await Promise.all([
    loader.loadAsync("public/scene.gltf"),
    loader.loadAsync("public/generated/scene-external.gltf"),
    loader.loadAsync("public/generated/scene.glb"),
  ]);
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
  if (!globalThis.__gltfExternalTexture) throw new Error("external glTF image texture did not decode");
  if (!globalThis.__gltfGlbLoaded) throw new Error("GLB container did not load through GLTFLoader");
  scene.add(gltf.scene);
  scene.add(externalGltf.scene);
  scene.add(glb.scene);
  globalThis.__gltfSmokeStage = "before-render";
  await renderer.renderAsync(scene, camera);
  globalThis.__gltfSmokeRendered = device !== null && renderer.isWebGPURenderer === true;
  globalThis.__gltfFeatureSmoke = instanced.isInstancedMesh && line.isLine && sprite.isSprite;
}).catch((error) => {
  globalThis.__gltfSmokeError = `${globalThis.__gltfSmokeStage}: ${String(error.stack || error)}`;
});

globalThis.HyperThreeGame = {
  update() {
    if (globalThis.__gltfSmokeError) throw new Error(globalThis.__gltfSmokeError.replace(/\n/g, " | "));
  },
  onStart() {
    if (globalThis.__gltfSmokeError) throw new Error(globalThis.__gltfSmokeError.replace(/\n/g, " | "));
    if (!globalThis.__gltfSmokeLoaded) throw new Error("GLTFLoader smoke did not settle");
    if (!globalThis.__gltfExternalTexture) throw new Error("external texture smoke did not settle");
    if (!globalThis.__gltfGlbLoaded) throw new Error("GLB smoke did not settle");
    if (!globalThis.__gltfResizeEvent) throw new Error("native resize event did not settle");
    if (!globalThis.__gltfFeatureSmoke) throw new Error("InstancedMesh/Line/Sprite smoke did not settle");
    if (!globalThis.__gltfReadbackSmoke) throw new Error("GPUBuffer readback smoke did not settle");
    if (!globalThis.__gltfResourceLifecycleSmoke) throw new Error("GPU resource lifecycle smoke did not settle");
    if (!globalThis.__gltfSmokeRendered) throw new Error("GLTF WebGPU render smoke did not settle");
  },
  onStop() {},
};
