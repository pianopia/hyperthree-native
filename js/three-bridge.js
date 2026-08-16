// Browser-free seam for Three.js scenes. The native host consumes a compact
// render list instead of exposing DOM/WebGL objects to the game bundle.
const hyperthreeRegisteredGeometryIds = new Set();

globalThis.HyperThreeNative = {
  version: "0.1.0",
  renderer: "wgpu-native",
  zeroCopyAssets: true,
  gpuDrivenCulling: false,
  setClearColor(r, g, b, a = 1) {
    __hyperthreeSetClearColor(r, g, b, a);
  },
  setCube(x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1, reserved = 0) {
    __hyperthreeSetCube(x, y, z, sx, sy, sz, rotationY, r, g, b, a, reserved);
  },
  beginFrame() {
    __hyperthreeBeginFrame();
  },
  pushCube(x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1, reserved = 0) {
    __hyperthreePushCube(x, y, z, sx, sy, sz, rotationY, r, g, b, a, reserved);
  },
  setCamera(px, py, pz, tx, ty, tz, fovY, near, far) {
    __hyperthreeSetCamera(px, py, pz, tx, ty, tz, fovY, near, far);
  },
  isKeyDown(code) {
    return __hyperthreeIsKeyDown(code);
  },
  isMouseButtonDown(button = 0) {
    return __hyperthreeIsMouseButtonDown(button);
  },
  getMousePosition() {
    return __hyperthreeGetMousePosition();
  },
  loadAsset(path) {
    return __hyperthreeLoadAsset(path);
  },
  drawAsset(path, meshIndex = 0, primitiveIndex = 0, options = {}) {
    __hyperthreeDrawAsset(
      path,
      meshIndex,
      primitiveIndex,
      options.x ?? 0,
      options.y ?? 0,
      options.z ?? 0,
      options.sx ?? 1,
      options.sy ?? 1,
      options.sz ?? 1,
      options.rotationY ?? 0,
      options.r ?? 0.1,
      options.g ?? 0.8,
      options.b ?? 0.95,
      options.a ?? 1,
    );
  },
  registerGeometry(id, positions, indices = []) {
    __hyperthreeRegisterGeometry(id, positions, indices);
  },
  pushGeometry(id, x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1) {
    __hyperthreePushGeometry(id, x, y, z, sx, sy, sz, rotationY, r, g, b, a);
  },
  syncThreeScene(scene, camera, options = {}) {
    const maxObjects = options.maxObjects ?? 4096;
    let renderedObjects = 0;
    let skippedObjects = 0;

    if (scene && typeof scene.updateMatrixWorld === "function") {
      scene.updateMatrixWorld(true);
    }
    if (camera && typeof camera.updateMatrixWorld === "function") {
      camera.updateMatrixWorld(true);
    }

    if (camera) {
      const position = camera.position || { x: 0, y: 0, z: 4 };
      const elements = camera.matrixWorld?.elements;
      const direction = elements
        ? [-elements[8], -elements[9], -elements[10]]
        : [0, 0, -1];
      HyperThreeNative.setCamera(
        position.x,
        position.y,
        position.z,
        position.x + direction[0],
        position.y + direction[1],
        position.z + direction[2],
        camera.fov ?? 60,
        camera.near ?? 0.1,
        camera.far ?? 100,
      );
    }

    HyperThreeNative.beginFrame();
    if (scene && typeof scene.traverse === "function") {
      scene.traverse((object) => {
        if (renderedObjects >= maxObjects || !object.visible || !object.isMesh) {
          return;
        }
        const geometryType = object.geometry?.type;
        const isCube = geometryType === "BoxGeometry" || geometryType === "BoxBufferGeometry";
        const isPlane = geometryType === "PlaneGeometry" || geometryType === "PlaneBufferGeometry";
        const isSphere = geometryType === "SphereGeometry" || geometryType === "SphereBufferGeometry";
        const positionAttribute = object.geometry?.attributes?.position;
        const customGeometryId = object.geometry?.id;
        const isCustom = !isCube && !isPlane && !isSphere
          && Number.isInteger(customGeometryId)
          && customGeometryId >= 0
          && positionAttribute?.array;
        if (!isCube && !isPlane && !isSphere && !isCustom) {
          skippedObjects += 1;
          return;
        }
        const elements = object.matrixWorld?.elements;
        const position = elements
          ? [elements[12], elements[13], elements[14]]
          : [object.position?.x ?? 0, object.position?.y ?? 0, object.position?.z ?? 0];
        const scale = object.scale || { x: 1, y: 1, z: 1 };
        const rotationY = object.rotation?.y ?? 0;
        const material = Array.isArray(object.material)
          ? object.material[0]
          : object.material;
        const color = material?.color || { r: 0.1, g: 0.8, b: 0.95 };
        const alpha = material?.opacity ?? 1;
        if (isCustom) {
          if (!hyperthreeRegisteredGeometryIds.has(customGeometryId)) {
            HyperThreeNative.registerGeometry(
              customGeometryId,
              positionAttribute.array,
              object.geometry.index?.array ?? [],
            );
            hyperthreeRegisteredGeometryIds.add(customGeometryId);
          }
          HyperThreeNative.pushGeometry(
            customGeometryId,
            position[0],
            position[1],
            position[2],
            scale.x,
            scale.y,
            scale.z,
            rotationY,
            color.r ?? 0.1,
            color.g ?? 0.8,
            color.b ?? 0.95,
            alpha,
          );
        } else {
          const push = isPlane
            ? HyperThreeNative.pushPlane
            : isSphere
              ? HyperThreeNative.pushSphere
              : HyperThreeNative.pushCube;
          push.call(
            HyperThreeNative,
            position[0],
            position[1],
            position[2],
            scale.x,
            scale.y,
            scale.z,
            rotationY,
            color.r ?? 0.1,
            color.g ?? 0.8,
            color.b ?? 0.95,
            alpha,
            0,
          );
        }
        renderedObjects += 1;
      });
    }
    return { renderedObjects, skippedObjects };
  },
  pushPlane(x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1, reserved = 0) {
    __hyperthreePushPlane(x, y, z, sx, sy, sz, rotationY, r, g, b, a, reserved);
  },
  pushSphere(x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1, reserved = 0) {
    __hyperthreePushSphere(x, y, z, sx, sy, sz, rotationY, r, g, b, a, reserved);
  },
};
