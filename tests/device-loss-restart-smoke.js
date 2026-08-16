const restartCount = globalThis.__hyperthreeNativeRestartCount;
globalThis.__deviceLossSmokeError = null;

if (restartCount === 0) {
  navigator.gpu.requestAdapter().then(async (adapter) => {
    const device = await adapter.requestDevice();
    device.destroy();
  }).catch((error) => {
    globalThis.__deviceLossSmokeError = String(error.stack || error);
  });
}

globalThis.HyperThreeGame = {
  update() {
    if (globalThis.__deviceLossSmokeError) throw new Error(globalThis.__deviceLossSmokeError);
  },
  onStart() {},
  onStop() {},
};
