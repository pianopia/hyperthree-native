import { defineConfig } from "vite";

export default defineConfig({
  build: {
    lib: {
      entry: "src/game.js",
      name: "HyperThreeGltfLoaderSmoke",
      formats: ["iife"],
      fileName: () => "game.js"
    },
    outDir: "dist",
    emptyOutDir: true
  }
});
