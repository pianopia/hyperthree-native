import * as THREE from "three";
import { WebGPURenderer } from "three/webgpu";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(60, 16 / 9, 0.1, 100);
camera.position.z = 4;
camera.lookAt(0, 0, 0);

globalThis.__gltfSmokeError = null;
globalThis.__gltfSmokeLoaded = false;
globalThis.__gltfSmokeRendered = false;
globalThis.__gltfSmokeStage = "before-adapter";

navigator.gpu.requestAdapter().then(async (adapter) => {
  globalThis.__gltfSmokeStage = "after-adapter";
  const device = await adapter.requestDevice();
  const renderer = new WebGPURenderer({ canvas: globalThis.__hyperthreeNativeCanvas, antialias: false });
  globalThis.__gltfSmokeStage = "before-renderer-init";
  await renderer.init();
  globalThis.__gltfSmokeStage = "before-gltf-load";
  const gltf = await new GLTFLoader().loadAsync("public/scene.gltf");
  globalThis.__gltfSmokeStage = "after-gltf-load";
  const skinned = gltf.scene.getObjectByProperty("isSkinnedMesh", true);
  if (!skinned || !skinned.skeleton || skinned.skeleton.bones.length !== 2) {
    throw new Error("GLTFLoader did not create the expected SkinnedMesh/Skeleton");
  }
  const mixer = new THREE.AnimationMixer(gltf.scene);
  mixer.clipAction(gltf.animations[0]).play();
  mixer.update(0.25);
  globalThis.__gltfSmokeLoaded = gltf.scene.children.length === 1 && gltf.animations.length === 1;
  scene.add(gltf.scene);
  globalThis.__gltfSmokeStage = "before-render";
  await renderer.renderAsync(scene, camera);
  globalThis.__gltfSmokeRendered = device !== null && renderer.isWebGPURenderer === true;
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
    if (!globalThis.__gltfSmokeRendered) throw new Error("GLTF WebGPU render smoke did not settle");
  },
  onStop() {},
};
