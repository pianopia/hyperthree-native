use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: Project,
    #[serde(default)]
    pub window: Window,
    #[serde(default)]
    pub build: Build,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default = "default_entry")]
    pub entry: PathBuf,
    #[serde(default = "default_output")]
    pub output: PathBuf,
}

#[derive(Debug, Default, Deserialize)]
pub struct Window {
    pub title: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Build {
    pub command: Option<String>,
}

fn default_entry() -> PathBuf {
    PathBuf::from("src/game.js")
}

fn default_output() -> PathBuf {
    PathBuf::from("dist/game.js")
}

pub fn load(root: impl AsRef<Path>) -> Result<(PathBuf, Manifest)> {
    let root = root.as_ref().canonicalize().with_context(|| {
        format!(
            "project directory does not exist: {}",
            root.as_ref().display()
        )
    })?;
    let manifest_path = root.join("hyperthree.toml");
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = toml::from_str(&source)
        .with_context(|| format!("invalid project manifest {}", manifest_path.display()))?;
    Ok((root, manifest))
}

pub fn init(root: impl AsRef<Path>) -> Result<()> {
    let root = root.as_ref();
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("public"))?;
    fs::write(root.join("hyperthree.toml"), INIT_MANIFEST)?;
    fs::write(root.join("package.json"), INIT_PACKAGE_JSON)?;
    fs::write(root.join("vite.config.js"), INIT_VITE_CONFIG)?;
    fs::write(root.join("src/game.js"), INIT_GAME_JS)?;
    fs::write(root.join("README.md"), INIT_README)?;
    Ok(())
}

pub fn build(root: impl AsRef<Path>, skip_command: bool) -> Result<PathBuf> {
    let (root, manifest) = load(root)?;
    let entry = root.join(&manifest.project.entry);
    let output = root.join(&manifest.project.output);
    anyhow::ensure!(
        entry.is_file(),
        "entry point not found: {}",
        entry.display()
    );

    if !skip_command {
        if let Some(command) = &manifest.build.command {
            log::info!("building {} with `{command}`", manifest.project.name);
            let status = if cfg!(target_os = "windows") {
                Command::new("cmd")
                    .args(["/C", command])
                    .current_dir(&root)
                    .status()?
            } else {
                Command::new("sh")
                    .args(["-c", command])
                    .current_dir(&root)
                    .status()?
            };
            anyhow::ensure!(status.success(), "project build command failed: {command}");
        }
    }

    if !output.is_file() {
        fs::create_dir_all(output.parent().unwrap_or(&root))?;
        fs::copy(&entry, &output).with_context(|| {
            format!(
                "failed to stage {} as {}",
                entry.display(),
                output.display()
            )
        })?;
        log::warn!("no build output found; staged the entry script directly");
    }
    Ok(output)
}

const INIT_MANIFEST: &str = r#"[project]
name = "my-hyperthree-game"
entry = "src/game.js"
output = "dist/game.js"

[window]
title = "My HyperThree Game"
width = 1280
height = 720

[build]
command = "npm run build"
"#;

const INIT_PACKAGE_JSON: &str = r#"{
  "name": "my-hyperthree-game",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "vite build"
  },
  "dependencies": {
    "three": "^0.179.0"
  },
  "devDependencies": {
    "vite": "^7.0.0"
  }
}
"#;

const INIT_VITE_CONFIG: &str = r#"import { defineConfig } from "vite";

export default defineConfig({
  build: {
    lib: {
      entry: "src/game.js",
      name: "HyperThreeGame",
      formats: ["iife"],
      fileName: () => "game.js"
    },
    outDir: "dist",
    emptyOutDir: true
  }
});
"#;

const INIT_GAME_JS: &str = r#"import * as THREE from "three";

const scene = new THREE.Scene();
scene.name = "AI-created HyperThree scene";
const camera = new THREE.PerspectiveCamera(60, 16 / 9, 0.1, 100);
camera.position.set(0, 0, 4);
camera.lookAt(0, 0, 0);
const cube = new THREE.Mesh(
  new THREE.BoxGeometry(1.4, 1.4, 1.4),
  new THREE.MeshBasicMaterial({ color: 0x19ccef }),
);
scene.add(cube);

// Keep the game state independent from DOM/WebGL. HyperThree will expose the
// native WebGPU bindings here as the runtime API is expanded.
globalThis.HyperThreeGame = {
  scene,
  camera,
  cube,
  targetObjects: 500000,
  renderer: "native-wgpu"
};

HyperThreeNative.setClearColor(0.015, 0.02, 0.05, 1.0);

let elapsed = 0;
globalThis.HyperThreeGame.update = (deltaSeconds) => {
  elapsed += deltaSeconds;
  cube.position.y = Math.sin(elapsed) * 0.25;
  cube.rotation.y = elapsed + 0.55;
  HyperThreeNative.syncThreeScene(scene, camera);
};

globalThis.HyperThreeGame.onStart = () => {};
globalThis.HyperThreeGame.onStop = () => {};
"#;

const INIT_README: &str = r#"# My HyperThree Game

## AI-first workflow

1. `npm install`
2. Ask your coding agent to edit `src/game.js` and keep the game state in `HyperThreeGame`.
3. `npm run build`
4. From the HyperThree Native checkout: `cargo run -- run --project /path/to/this/project`

`hyperthree.toml` is the stable contract between the generated game and the
native host. Keep browser-only DOM/WebGL calls out of the native entry until
the corresponding native bridge is enabled.
"#;
