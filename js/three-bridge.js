// Browser-free seam for the future Three.js WebGPU renderer adapter.
// The native host can replace these functions with V8-bound WebGPU objects.
globalThis.HyperThreeNative = {
  version: "0.1.0",
  renderer: "wgpu-native",
  zeroCopyAssets: true,
  gpuDrivenCulling: false,
};

