// This entry point intentionally uses the same JavaScript-facing shape that a
// Three.js game can consume. The native host evaluates it without a browser.
globalThis.sceneConfig = {
  title: "HyperThree Native smoke test",
  clearColor: [0.025, 0.04, 0.09, 1.0],
  targetObjects: 500000,
};

