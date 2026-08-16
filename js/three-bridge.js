// Browser-free seam for Three.js scenes. The native host consumes a compact
// render list instead of exposing DOM/WebGL objects to the game bundle.
const hyperthreeRegisteredGeometryIds = new Set();

globalThis.HyperThreeNative = {
  version: "0.1.0",
  renderer: "wgpu-native",
  // Standard Three.js WebGPURenderer receives the native canvas directly.
  // syncThreeScene() remains below as an explicit migration/diagnostic bridge.
  canvas: globalThis.__hyperthreeNativeCanvas,
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
  pushParticle(x, y, z, size, r, g, b, a = 1, emissiveR = 1, emissiveG = 1, emissiveB = 1) {
    __hyperthreePushParticle(x, y, z, size, r, g, b, a, emissiveR, emissiveG, emissiveB);
  },
  pushCube(x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1, reserved = 0) {
    __hyperthreePushCube(x, y, z, sx, sy, sz, rotationY, r, g, b, a, reserved);
  },
  setCamera(px, py, pz, tx, ty, tz, fovY, near, far) {
    __hyperthreeSetCamera(px, py, pz, tx, ty, tz, fovY, near, far);
  },
  setOrthographicCamera(px, py, pz, tx, ty, tz, left, right, top, bottom, near, far) {
    __hyperthreeSetOrthographicCamera(
      px, py, pz, tx, ty, tz, left, right, top, bottom, near, far,
    );
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
  registerGeometry(id, positions, indices = [], uvs = [], normals = []) {
    __hyperthreeRegisterGeometry(id, positions, indices, uvs, normals);
  },
  pushGeometry(id, x, y, z, sx, sy, sz, rotationY, r, g, b, a = 1, textureId = -1) {
    __hyperthreePushGeometry(id, x, y, z, sx, sy, sz, rotationY, r, g, b, a, textureId);
  },
  pushGeometryMaterial(
    id, x, y, z, sx, sy, sz, rotationY,
    r, g, b, a = 1, textureId = -1,
    metallic = 0, roughness = 0.65,
    emissiveR = 0, emissiveG = 0, emissiveB = 0, unlit = false,
  ) {
    __hyperthreePushGeometryMaterial(
      id, x, y, z, sx, sy, sz, rotationY,
      r, g, b, a, textureId, metallic, roughness,
      emissiveR, emissiveG, emissiveB, unlit ? 1 : 0,
    );
  },
  pushGeometryMatrixMaterial(
    id, matrix, r, g, b, a = 1, textureId = -1,
    metallic = 0, roughness = 0.65,
    emissiveR = 0, emissiveG = 0, emissiveB = 0, unlit = false,
  ) {
    __hyperthreePushGeometryMatrixMaterial(
      id, matrix, r, g, b, a, textureId, metallic, roughness,
      emissiveR, emissiveG, emissiveB, unlit ? 1 : 0,
    );
  },
  pushPrimitiveMatrixMaterial(
    kind, matrix, r, g, b, a = 1, textureId = -1,
    metallic = 0, roughness = 0.65,
    emissiveR = 0, emissiveG = 0, emissiveB = 0, unlit = false,
  ) {
    __hyperthreePushPrimitiveMatrixMaterial(
      kind, matrix, r, g, b, a, textureId, metallic, roughness,
      emissiveR, emissiveG, emissiveB, unlit ? 1 : 0,
    );
  },
  setDirectionalLight(direction, color, intensity = 2.5, ambient = { r: 0.08, g: 0.1, b: 0.14 }) {
    __hyperthreeSetDirectionalLight(
      direction.x, direction.y, direction.z,
      color.r, color.g, color.b,
      intensity, ambient.r, ambient.g, ambient.b,
    );
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
      const target = [
        position.x + direction[0],
        position.y + direction[1],
        position.z + direction[2],
      ];
      if (camera.isOrthographicCamera || camera.type === "OrthographicCamera") {
        HyperThreeNative.setOrthographicCamera(
          position.x,
          position.y,
          position.z,
          target[0],
          target[1],
          target[2],
          camera.left ?? -1,
          camera.right ?? 1,
          camera.top ?? 1,
          camera.bottom ?? -1,
          camera.near ?? 0.1,
          camera.far ?? 100,
        );
      } else {
        HyperThreeNative.setCamera(
          position.x,
          position.y,
          position.z,
          target[0],
          target[1],
          target[2],
          camera.fov ?? 60,
          camera.near ?? 0.1,
          camera.far ?? 100,
        );
      }
    }

    HyperThreeNative.beginFrame();
    if (scene && typeof scene.traverse === "function") {
      scene.traverse((object) => {
        if (!object.visible) {
          return;
        }
        if (object.isDirectionalLight) {
          const lightPosition = object.position || { x: -0.35, y: -0.8, z: -0.45 };
          const lightColor = object.color || { r: 1, g: 1, b: 1 };
          HyperThreeNative.setDirectionalLight(
            { x: lightPosition.x, y: lightPosition.y, z: lightPosition.z },
            { r: lightColor.r ?? 1, g: lightColor.g ?? 1, b: lightColor.b ?? 1 },
            object.intensity ?? 2.5,
          );
          return;
        }
        if (object.isPoints) {
          const positionAttribute = object.geometry?.attributes?.position;
          if (!positionAttribute?.array) {
            skippedObjects += 1;
            return;
          }
          const elements = object.matrixWorld?.elements;
          const material = Array.isArray(object.material)
            ? object.material[0]
            : object.material;
          const color = material?.color || { r: 1, g: 1, b: 1 };
          const alpha = material?.opacity ?? 1;
          const size = material?.size ?? 0.08;
          const intensity = material?.userData?.hyperthreeEmissiveIntensity ?? 1;
          for (let index = 0; index + 2 < positionAttribute.array.length && renderedObjects < maxObjects; index += 3) {
            const x = positionAttribute.array[index];
            const y = positionAttribute.array[index + 1];
            const z = positionAttribute.array[index + 2];
            const world = elements
              ? [
                elements[0] * x + elements[4] * y + elements[8] * z + elements[12],
                elements[1] * x + elements[5] * y + elements[9] * z + elements[13],
                elements[2] * x + elements[6] * y + elements[10] * z + elements[14],
              ]
              : [
                x + (object.position?.x ?? 0),
                y + (object.position?.y ?? 0),
                z + (object.position?.z ?? 0),
              ];
            HyperThreeNative.pushParticle(
              world[0], world[1], world[2], size,
              color.r ?? 1, color.g ?? 1, color.b ?? 1, alpha,
              (color.r ?? 1) * intensity,
              (color.g ?? 1) * intensity,
              (color.b ?? 1) * intensity,
            );
            renderedObjects += 1;
          }
          return;
        }
        if (renderedObjects >= maxObjects || !object.isMesh) {
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
        const modelMatrix = elements
          ? Array.from(elements)
          : [
            scale.x, 0, 0, 0, 0, scale.y, 0, 0, 0, 0, scale.z, 0,
            position[0], position[1], position[2], 1,
          ];
        if (elements) {
          const matrixScale = [
            Math.hypot(modelMatrix[0], modelMatrix[1], modelMatrix[2]),
            Math.hypot(modelMatrix[4], modelMatrix[5], modelMatrix[6]),
            Math.hypot(modelMatrix[8], modelMatrix[9], modelMatrix[10]),
          ];
          if (matrixScale.every((value) => Math.abs(value - 1) < 1e-5)) {
            modelMatrix[0] *= scale.x;
            modelMatrix[1] *= scale.x;
            modelMatrix[2] *= scale.x;
            modelMatrix[4] *= scale.y;
            modelMatrix[5] *= scale.y;
            modelMatrix[6] *= scale.y;
            modelMatrix[8] *= scale.z;
            modelMatrix[9] *= scale.z;
            modelMatrix[10] *= scale.z;
          }
        }
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
              object.geometry.attributes.uv?.array ?? [],
              object.geometry.attributes.normal?.array ?? [],
            );
            hyperthreeRegisteredGeometryIds.add(customGeometryId);
          }
          const isStandard = material?.isMeshStandardMaterial || material?.isMeshPhysicalMaterial;
          HyperThreeNative.pushGeometryMatrixMaterial(
            customGeometryId,
            modelMatrix,
            color.r ?? 0.1,
            color.g ?? 0.8,
            color.b ?? 0.95,
            alpha,
            material?.userData?.hyperthreeTextureId ?? -1,
            material?.metalness ?? 0,
            material?.roughness ?? 0.65,
            material?.emissive?.r ?? 0,
            material?.emissive?.g ?? 0,
            material?.emissive?.b ?? 0,
            !isStandard && !!material?.isMeshBasicMaterial,
          );
        } else {
          const primitiveKind = isPlane ? 1 : isSphere ? 2 : 0;
          const isStandard = material?.isMeshStandardMaterial || material?.isMeshPhysicalMaterial;
          HyperThreeNative.pushPrimitiveMatrixMaterial(
            primitiveKind,
            modelMatrix,
            color.r ?? 0.1,
            color.g ?? 0.8,
            color.b ?? 0.95,
            alpha,
            material?.userData?.hyperthreeTextureId ?? -1,
            material?.metalness ?? 0,
            material?.roughness ?? 0.65,
            material?.emissive?.r ?? 0,
            material?.emissive?.g ?? 0,
            material?.emissive?.b ?? 0,
            !isStandard && !!material?.isMeshBasicMaterial,
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
