// This entry point intentionally uses the same JavaScript-facing shape that a
// Three.js game can consume. The native host evaluates it without a browser.
globalThis.sceneConfig = {
  title: "HyperThree Native smoke test",
  clearColor: [0.025, 0.04, 0.09, 1.0],
  targetObjects: 500000,
};

HyperThreeNative.setClearColor(0.025, 0.04, 0.09, 1.0);
HyperThreeNative.setCamera(0, 0, 4, 0, 0, 0, 60, 0.1, 100);
HyperThreeNative.setCube(0, 0, 0, 1, 1, 1, 0.55, 0.1, 0.8, 0.95, 1, 0);
