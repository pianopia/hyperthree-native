// This entry point intentionally uses the same JavaScript-facing shape that a
// Three.js game can consume. The native host evaluates it without a browser.
globalThis.sceneConfig = {
  title: "HyperThree Native smoke test",
  clearColor: [0.025, 0.04, 0.09, 1.0],
  targetObjects: 500000,
};

HyperThreeNative.setClearColor(0.025, 0.04, 0.09, 1.0);
HyperThreeNative.setTriangleColor(0, 0.08, 0.85, 0.78);
HyperThreeNative.setTriangleColor(1, 0.16, 0.35, 0.98);
HyperThreeNative.setTriangleColor(2, 0.75, 0.25, 0.96);
