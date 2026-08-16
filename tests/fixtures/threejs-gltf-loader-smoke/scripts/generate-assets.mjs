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

const makeWav = () => {
  const sampleRate = 8000;
  const samples = [0, 8192, -8192, 0];
  const output = Buffer.alloc(44 + samples.length * 2);
  output.write("RIFF", 0);
  output.writeUInt32LE(36 + samples.length * 2, 4);
  output.write("WAVEfmt ", 8);
  output.writeUInt32LE(16, 16);
  output.writeUInt16LE(1, 20);
  output.writeUInt16LE(1, 22);
  output.writeUInt32LE(sampleRate, 24);
  output.writeUInt32LE(sampleRate * 2, 28);
  output.writeUInt16LE(2, 32);
  output.writeUInt16LE(16, 34);
  output.write("data", 36);
  output.writeUInt32LE(samples.length * 2, 40);
  samples.forEach((sample, index) => output.writeInt16LE(sample, 44 + index * 2));
  return output;
};

const basisLzKtx2 = Buffer.from(
  "q0tUWCAyMLsNChoKAAAAAAEAAAAIAAAACAAAAAAAAAAAAAAAAQAAAAEAAAABAAAAaAAAADwAAACkAAAARAAAAOgAAAAAAAAAjAAAAAAAAAB0AQAAAAAAAAMAAAAAAAAAAAAAAAAAAAA8AAAAAAAAAAIAOACjAQIAAwMAAAgIAAAAAAAAAAA/AAAAAAAAAAAA/////0AAPw8AAAAAAAAAAP////9AAAAAS1RYd3JpdGVyAGt0eCBjcmVhdGUgdjUuMC5fX2RlZmF1bHRfXyAvIGxpYmt4IHY1LjAuX19kZWZhdWx0X18AAQIAAgAtAAAACQAAAC4AAAAAAAAAAAAAAAAAAAABAAAAAQAAAAIAAAABwAQAAAAAAAACBJgbIAAAAAjDNpE+kQBgAgAAAAAAAIEATAEQAAAAACBZwD2sqqqqUlVVVQUUwEQAAAAAAAASQQCYAAAAAAAAQBgCogQMAAAAg3Z7SQSiIABMAAgAAAAAIAIBBkwO",
  "base64",
);

const dracoBox = Buffer.from(
  "RFJBQ08CAgEBAAAACAwBCwAAA19bCgEBEFUEXOONRgL/AAAAAQABAAkDAAECAQEJAwAAAwEBAQADAwEwARADACSWEwokBAAAAAD/BwAAAAAAvwAAAL8AAAC/AACAPwsGAwEBAQEBQAEA/wAAAH8AAAD/AqFBCAAA",
  "base64",
);

const makeKtx2Uastc = () => {
  const block = Buffer.from([0xf7, 0x1f, 0x08, 0xe4, 0x1f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
  const levelDataOffset = 152;
  const output = Buffer.alloc(levelDataOffset + block.length);
  const writeU32 = (offset, value) => output.writeUInt32LE(value, offset);
  const writeU64 = (offset, value) => output.writeBigUInt64LE(BigInt(value), offset);

  output.set(Buffer.from([0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a]), 0);
  writeU32(12, 0); // VK_FORMAT_UNDEFINED; DFD identifies UASTC LDR.
  writeU32(16, 1);
  writeU32(20, 4);
  writeU32(24, 4);
  writeU32(28, 0);
  writeU32(32, 0);
  writeU32(36, 1);
  writeU32(40, 1);
  writeU32(44, 0); // KHR_SUPERCOMPRESSION_NONE; raw 16-byte UASTC block.
  writeU32(48, 104);
  writeU32(52, 44);
  writeU32(56, 0);
  writeU32(60, 0);
  writeU64(64, 0);
  writeU64(72, 0);
  writeU64(80, levelDataOffset);
  writeU64(88, block.length);
  writeU64(96, block.length);

  // KHR_DF_MODEL_UASTC, 4x4 block, 128-bit payload, RGB channel.
  output.set(Buffer.from([
    0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x28, 0x00,
    0xa6, 0x01, 0x02, 0x00, 0x03, 0x03, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff,
  ]), 104);
  output.set(block, levelDataOffset);
  return output;
};

const makeKtx2Bc1 = () => {
  const width = 4;
  const height = 4;
  const levelData = Buffer.from([0x00, 0xf8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
  const dfdBlockLength = 8 + 16 + 16;
  const dfdLength = 4 + dfdBlockLength;
  const levelDataOffset = 80 + 24 + dfdLength;
  const output = Buffer.alloc(levelDataOffset + levelData.length);
  const writeU32 = (offset, value) => output.writeUInt32LE(value, offset);
  const writeU64 = (offset, value) => output.writeBigUInt64LE(BigInt(value), offset);

  output.set(Buffer.from([0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a]), 0);
  writeU32(12, 134); // VK_FORMAT_BC1_RGBA_SRGB_BLOCK
  writeU32(16, 1);
  writeU32(20, width);
  writeU32(24, height);
  writeU32(28, 0);
  writeU32(32, 0);
  writeU32(36, 1);
  writeU32(40, 1);
  writeU32(44, 0); // KHR_SUPERCOMPRESSION_NONE
  writeU32(48, 104);
  writeU32(52, dfdLength);
  writeU32(56, 0);
  writeU32(60, 0);
  writeU64(64, 0);
  writeU64(72, 0);

  writeU64(80, levelDataOffset);
  writeU64(88, levelData.length);
  writeU64(96, levelData.length);

  writeU32(104, dfdLength);
  output[112] = 2;
  output.writeUInt16LE(dfdBlockLength, 114);
  output[116] = 0x80; // BC1A color model
  output[117] = 1; // BT.709 primaries
  output[118] = 2; // sRGB transfer function
  output[119] = 0;
  output.set([3, 3, 0, 0], 120); // 4x4x1x1 texel block dimensions minus one
  output[124] = levelData.length;
  output[125] = 0;
  const sampleOffset = 132;
  output[sampleOffset + 2] = 63; // 64-bit compressed sample
  output[sampleOffset + 3] = 0;
  writeU32(sampleOffset + 12, 0xffffffff);
  output.set(levelData, levelDataOffset);
  return output;
};

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
await writeFile(new URL("tone.wav", generated), makeWav());
await writeFile(new URL("scene-external.gltf", generated), JSON.stringify(externalDocument, null, 2));

const ktx2Document = JSON.parse(JSON.stringify(externalDocument));
ktx2Document.extensionsUsed = ["KHR_texture_basisu"];
ktx2Document.extensionsRequired = ["KHR_texture_basisu"];
ktx2Document.images = [{ uri: "scene-ktx2.ktx2", mimeType: "image/ktx2" }];
ktx2Document.textures = [{ extensions: { KHR_texture_basisu: { source: 0 } } }];
await writeFile(new URL("scene-ktx2.ktx2", generated), makeKtx2Bc1());
await writeFile(new URL("scene-ktx2.gltf", generated), JSON.stringify(ktx2Document, null, 2));

const uastcKtx2Document = JSON.parse(JSON.stringify(externalDocument));
uastcKtx2Document.extensionsUsed = ["KHR_texture_basisu"];
uastcKtx2Document.extensionsRequired = ["KHR_texture_basisu"];
uastcKtx2Document.images = [{ uri: "scene-ktx2-uastc.ktx2", mimeType: "image/ktx2" }];
uastcKtx2Document.textures = [{ extensions: { KHR_texture_basisu: { source: 0 } } }];
await writeFile(new URL("scene-ktx2-uastc.ktx2", generated), makeKtx2Uastc());
await writeFile(new URL("scene-ktx2-uastc.gltf", generated), JSON.stringify(uastcKtx2Document, null, 2));

const basisKtx2Document = JSON.parse(JSON.stringify(externalDocument));
basisKtx2Document.extensionsUsed = ["KHR_texture_basisu"];
basisKtx2Document.extensionsRequired = ["KHR_texture_basisu"];
basisKtx2Document.images = [{ uri: "scene-ktx2-basis.ktx2", mimeType: "image/ktx2" }];
basisKtx2Document.textures = [{ extensions: { KHR_texture_basisu: { source: 0 } } }];
await writeFile(new URL("scene-ktx2-basis.ktx2", generated), basisLzKtx2);
await writeFile(new URL("scene-ktx2-basis.gltf", generated), JSON.stringify(basisKtx2Document, null, 2));

const dracoDocument = {
  asset: { version: "2.0", generator: "Khronos glTF Sample Assets" },
  scene: 0,
  scenes: [{ nodes: [0] }],
  nodes: [{ mesh: 0 }],
  meshes: [{ primitives: [{
    attributes: { NORMAL: 0, POSITION: 1 },
    indices: 2,
    mode: 4,
    material: 0,
    extensions: {
      KHR_draco_mesh_compression: {
        bufferView: 0,
        attributes: { NORMAL: 0, POSITION: 1 },
      },
    },
  }] }],
  accessors: [
    { componentType: 5126, count: 24, type: "VEC3", max: [1.007843137254902, 1.007843137254902, 1.007843137254902], min: [-1.007843137254902, -1.007843137254902, -1.007843137254902] },
    { componentType: 5126, count: 24, type: "VEC3", max: [0.5004885197850513, 0.5004885197850513, 0.5004885197850513], min: [-0.5004885197850513, -0.5004885197850513, -0.5004885197850513] },
    { componentType: 5123, count: 36, type: "SCALAR", max: [23], min: [0] },
  ],
  materials: [{ pbrMetallicRoughness: { baseColorFactor: [0.8, 0.05, 0.02, 1], metallicFactor: 0.1, roughnessFactor: 0.8 } }],
  buffers: [{ byteLength: dracoBox.length, uri: "scene-draco.bin" }],
  bufferViews: [{ buffer: 0, byteOffset: 0, byteLength: dracoBox.length }],
  extensionsUsed: ["KHR_draco_mesh_compression"],
  extensionsRequired: ["KHR_draco_mesh_compression"],
};
await writeFile(new URL("scene-draco.bin", generated), dracoBox);
await writeFile(new URL("scene-draco.gltf", generated), JSON.stringify(dracoDocument, null, 2));

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

const meshoptVertices = Buffer.from(
  "oAAAATwAAAD//wE8AAAAfn0AAAEMAAAA/wEMAAAAfgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
  "base64",
);
const meshoptIndices = Buffer.from("4fAAdodWZ3iphmWJaJgBaQAA", "base64");
const meshoptSource = Buffer.concat([meshoptVertices, meshoptIndices]);
const meshoptDocument = {
  asset: { version: "2.0" },
  extensionsUsed: ["EXT_meshopt_compression"],
  extensionsRequired: ["EXT_meshopt_compression"],
  buffers: [
    { byteLength: meshoptSource.length, uri: "scene-meshopt.bin" },
    { byteLength: 42, uri: "scene-meshopt-fallback.bin" },
  ],
  bufferViews: [
    {
      buffer: 1,
      byteOffset: 0,
      byteLength: 36,
      extensions: {
        EXT_meshopt_compression: {
          buffer: 0,
          byteOffset: 0,
          byteLength: meshoptVertices.length,
          byteStride: 12,
          count: 3,
          mode: "ATTRIBUTES",
          filter: "NONE",
        },
      },
    },
    {
      buffer: 1,
      byteOffset: 36,
      byteLength: 6,
      extensions: {
        EXT_meshopt_compression: {
          buffer: 0,
          byteOffset: meshoptVertices.length,
          byteLength: meshoptIndices.length,
          byteStride: 2,
          count: 3,
          mode: "TRIANGLES",
        },
      },
    },
  ],
  accessors: [
    { bufferView: 0, componentType: 5126, count: 3, type: "VEC3", min: [-0.6, 0, 0], max: [0.6, 1.2, 0] },
    { bufferView: 1, componentType: 5123, count: 3, type: "SCALAR" },
  ],
  meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 1, mode: 4 }] }],
  nodes: [{ mesh: 0 }],
  scenes: [{ nodes: [0] }],
  scene: 0,
};
await writeFile(new URL("scene-meshopt.bin", generated), meshoptSource);
await writeFile(new URL("scene-meshopt-fallback.bin", generated), Buffer.alloc(42));
await writeFile(new URL("scene-meshopt.gltf", generated), JSON.stringify(meshoptDocument, null, 2));
