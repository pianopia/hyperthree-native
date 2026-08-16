// Browser-free seam for the future Three.js WebGPU renderer adapter.
// The native host can replace these functions with V8-bound WebGPU objects.
globalThis.HyperThreeNative = {
  version: "0.1.0",
  renderer: "wgpu-native",
  zeroCopyAssets: true,
  gpuDrivenCulling: false,
  setClearColor(r, g, b, a = 1) {
    __hyperthreeSetClearColor(r, g, b, a);
  },
  setTriangleColor(index, r, g, b) {
    __hyperthreeSetTriangleColor(index, r, g, b);
  },
};
