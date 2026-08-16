import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

const root = new URL("../", import.meta.url);
const generated = new URL("public/generated/", root);
await mkdir(generated, { recursive: true });

const geometry = Buffer.alloc(160);
const positions = [
  -0.6, 0, 0,
  0.6, 0, 0,
  0, 1.2, 0,
];
positions.forEach((value, index) => geometry.writeFloatLE(value, index * 4));
for (let index = 0; index < 12; index += 1) geometry[36 + index] = 0;
for (let vertex = 0; vertex < 3; vertex += 1) geometry.writeFloatLE(1, 48 + vertex * 16);
geometry.writeFloatLE(0, 96);
geometry.writeFloatLE(1, 100);
geometry.writeFloatLE(0, 104);
geometry.writeFloatLE(0, 108);
geometry.writeFloatLE(0, 112);
geometry.writeFloatLE(1, 116);
geometry.writeFloatLE(Math.sin(Math.PI / 8), 120);
geometry.writeFloatLE(Math.cos(Math.PI / 8), 124);
geometry.writeFloatLE(0, 128);
geometry.writeFloatLE(0, 132);
geometry.writeFloatLE(0, 136);
geometry.writeFloatLE(0, 140);
geometry.writeFloatLE(1, 144);
geometry.writeFloatLE(0.5, 148);
geometry.writeFloatLE(1, 152);
geometry.writeFloatLE(1, 156);

const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
  "base64",
);

const makeDocument = ({ embeddedImage, glb }) => {
  const buffer = glb ? { byteLength: geometry.length + png.length } : {
    byteLength: geometry.length,
    uri: "scene.bin",
  };
  const image = glb
    ? { bufferView: 6, mimeType: "image/png" }
    : { uri: "texture.png", mimeType: "image/png" };
  return {
    asset: { version: "2.0" },
    buffers: [buffer],
    bufferViews: [
      { buffer: 0, byteOffset: 0, byteLength: 36 },
      { buffer: 0, byteOffset: 36, byteLength: 12 },
      { buffer: 0, byteOffset: 48, byteLength: 48 },
      { buffer: 0, byteOffset: 96, byteLength: 8 },
      { buffer: 0, byteOffset: 104, byteLength: 32 },
      { buffer: 0, byteOffset: 136, byteLength: 24 },
      ...(glb ? [{ buffer: 0, byteOffset: 160, byteLength: png.length }] : []),
    ],
    accessors: [
      { bufferView: 0, componentType: 5126, count: 3, type: "VEC3" },
      { bufferView: 1, componentType: 5121, count: 3, type: "VEC4" },
      { bufferView: 2, componentType: 5126, count: 3, type: "VEC4" },
      { bufferView: 3, componentType: 5126, count: 2, type: "SCALAR", min: [0], max: [1] },
      { bufferView: 4, componentType: 5126, count: 2, type: "VEC4" },
      { bufferView: 5, componentType: 5126, count: 3, type: "VEC2" },
    ],
    images: [image],
    textures: [{ source: 0 }],
    materials: [{ pbrMetallicRoughness: {
      baseColorFactor: [0.2, 0.7, 1, 1],
      baseColorTexture: { index: 0 },
      metallicFactor: 0.1,
      roughnessFactor: 0.7,
    } }],
    meshes: [{ primitives: [{
      attributes: { POSITION: 0, JOINTS_0: 1, WEIGHTS_0: 2, TEXCOORD_0: 5 },
      material: 0,
      mode: 4,
    }] }],
    nodes: [{ mesh: 0, skin: 0 }, { name: "RootJoint", children: [2] }, { name: "AnimatedJoint", translation: [0, 0.6, 0] }],
    skins: [{ joints: [1, 2], skeleton: 1 }],
    animations: [{ name: "joint-rotation", samplers: [{ input: 3, output: 4, interpolation: "LINEAR" }], channels: [{ sampler: 0, target: { node: 2, path: "rotation" } }] }],
    scenes: [{ nodes: [0] }],
    scene: 0,
  };
};

const externalDocument = makeDocument({ glb: false });
await writeFile(new URL("scene.bin", generated), geometry);
await writeFile(new URL("texture.png", generated), png);
await writeFile(new URL("scene-external.gltf", generated), JSON.stringify(externalDocument, null, 2));

const glbDocument = makeDocument({ glb: true });
const json = Buffer.from(JSON.stringify(glbDocument));
const jsonPadding = Buffer.alloc((4 - (json.length % 4)) % 4, 0x20);
const bin = Buffer.concat([geometry, png, Buffer.alloc((4 - ((geometry.length + png.length) % 4)) % 4)]);
const header = Buffer.alloc(12);
header.writeUInt32LE(0x46546c67, 0);
header.writeUInt32LE(2, 4);
header.writeUInt32LE(12 + 8 + json.length + jsonPadding.length + 8 + bin.length, 8);
const jsonHeader = Buffer.alloc(8);
jsonHeader.writeUInt32LE(json.length + jsonPadding.length, 0);
jsonHeader.writeUInt32LE(0x4e4f534a, 4);
const binHeader = Buffer.alloc(8);
binHeader.writeUInt32LE(bin.length, 0);
binHeader.writeUInt32LE(0x004e4942, 4);
await writeFile(new URL("scene.glb", generated), Buffer.concat([header, jsonHeader, json, jsonPadding, binHeader, bin]));
