mod asset;
mod bridge;
mod js_runtime;
mod platform;
mod project;
mod renderer;
mod webgpu;

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, Parser, Subcommand};
use js_runtime::JsRuntime;
use project::Manifest;
use renderer::Renderer;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use winit::{
    event::{Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::WindowBuilder,
};

#[derive(Debug, Parser)]
#[command(name = "hyperthree-native", about = "Native Three.js host prototype")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// JavaScript entry point executed before the native window starts.
    #[arg(long, default_value = "js/game.js", global = true)]
    script: PathBuf,

    /// Optional binary asset to mmap as a zero-copy loading smoke test.
    #[arg(long, global = true)]
    asset: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create an AI-friendly HyperThree project scaffold.
    Init { path: PathBuf },
    /// Run the configured npm build and validate the native output.
    Build(BuildArgs),
    /// Build when needed and launch the project in the native host.
    Run(RunArgs),
    /// Print host and visible GPU backend diagnostics without opening a window.
    Diagnostics,
}

#[derive(Debug, ClapArgs)]
struct BuildArgs {
    #[arg(long, default_value = ".")]
    project: PathBuf,
    #[arg(long)]
    skip_command: bool,
}

#[derive(Debug, ClapArgs)]
struct RunArgs {
    #[arg(long, default_value = ".")]
    project: PathBuf,
    #[arg(long)]
    skip_build: bool,
}

fn project_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    }
}

fn runtime_root(script: &Path) -> PathBuf {
    let mut candidate = script
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    loop {
        if candidate.join("hyperthree.toml").is_file() || candidate.join("package.json").is_file() {
            return candidate;
        }
        let Some(parent) = candidate.parent() else {
            return candidate;
        };
        if parent == candidate {
            return candidate;
        }
        candidate = parent.to_path_buf();
    }
}

fn run_native(
    script: PathBuf,
    asset_path: Option<PathBuf>,
    manifest: Option<Manifest>,
    asset_root: PathBuf,
) -> Result<()> {
    let render_state = bridge::NativeRenderState::shared();
    let input_state = bridge::NativeInputState::shared();
    let event_loop = EventLoop::new().context("failed to create native event loop")?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(
                manifest
                    .as_ref()
                    .and_then(|manifest| manifest.window.title.as_deref())
                    .unwrap_or("HyperThree Native"),
            )
            .with_inner_size(winit::dpi::PhysicalSize::new(
                manifest
                    .as_ref()
                    .and_then(|manifest| manifest.window.width)
                    .unwrap_or(1280),
                manifest
                    .as_ref()
                    .and_then(|manifest| manifest.window.height)
                    .unwrap_or(720),
            ))
            .build(&event_loop)
            .context("failed to create native window")?,
    );
    let mut renderer = pollster::block_on(Renderer::new(window, render_state.clone()))?;
    let mut runtime = JsRuntime::new_with_gpu(
        render_state.clone(),
        input_state.clone(),
        asset_root,
        Some(renderer.webgpu_context()),
    )?;
    runtime.execute_source(include_str!("../js/three-bridge.js"))?;
    runtime.execute_file(&script)?;
    runtime.execute_start()?;
    log::info!("executed JavaScript entry point: {}", script.display());
    let snapshot = render_state
        .lock()
        .expect("render state mutex should not be poisoned")
        .snapshot();
    log::info!(
        "native bridge state: clear={:?}, cubes={}, camera={:?}",
        snapshot.clear_color,
        snapshot.cubes.len(),
        snapshot.camera
    );

    if let Some(asset_path) = asset_path {
        let asset = asset::MappedAsset::open(&asset_path)?;
        log::info!(
            "memory-mapped asset: {} ({} bytes)",
            asset_path.display(),
            asset.len()
        );
        // Keep the mapping alive through the startup path; native decoders can
        // consume asset.bytes() here without a JS heap copy.
        let _ = asset.bytes().len();
    }

    let mut last_frame = Instant::now();

    event_loop.run(move |event, event_loop| {
        event_loop.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { window_id, event } if window_id == renderer.window.id() => {
                match event {
                    WindowEvent::CloseRequested => {
                        if let Err(error) = runtime.execute_shutdown() {
                            log::error!("JavaScript shutdown callback failed: {error:#}");
                        }
                        event_loop.exit();
                    }
                    WindowEvent::Resized(size) => renderer.resize(size),
                    WindowEvent::Focused(false) => {
                        input_state
                            .lock()
                            .expect("input state mutex should not be poisoned")
                            .clear();
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            input_state
                                .lock()
                                .expect("input state mutex should not be poisoned")
                                .set_key(format!("{code:?}"), event.state.is_pressed());
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        input_state
                            .lock()
                            .expect("input state mutex should not be poisoned")
                            .set_mouse_position(position.x, position.y);
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if let Some(button) = mouse_button_id(button) {
                            input_state
                                .lock()
                                .expect("input state mutex should not be poisoned")
                                .set_mouse_button(button, state.is_pressed());
                        }
                    }
                    WindowEvent::RedrawRequested => match renderer.render() {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            renderer.resize(renderer.window.inner_size())
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(wgpu::SurfaceError::Timeout) => {
                            log::warn!("surface timeout; skipping frame")
                        }
                    },
                    _ => {}
                }
            }
            Event::AboutToWait => {
                let now = Instant::now();
                let delta_seconds = now.duration_since(last_frame).as_secs_f64().clamp(0.0, 0.1);
                last_frame = now;
                if let Err(error) = runtime.execute_frame(delta_seconds) {
                    log::error!("JavaScript frame update failed: {error:#}");
                    if let Err(shutdown_error) = runtime.execute_shutdown() {
                        log::error!("JavaScript shutdown callback failed: {shutdown_error:#}");
                    }
                    event_loop.exit();
                } else {
                    renderer.window.request_redraw();
                }
            }
            _ => {}
        }
    })?;
    Ok(())
}

fn mouse_button_id(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        MouseButton::Back => Some(3),
        MouseButton::Forward => Some(4),
        MouseButton::Other(_) => None,
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Init { path }) => {
            project::init(&path)?;
            println!("created HyperThree project at {}", path.display());
            Ok(())
        }
        Some(Command::Build(args)) => {
            let output = project::build(&args.project, args.skip_command)?;
            println!("native bundle ready: {}", output.display());
            Ok(())
        }
        Some(Command::Run(args)) => {
            let (root, manifest) = project::load(&args.project)?;
            let script = if args.skip_build {
                root.join(&manifest.project.output)
            } else {
                project::build(&root, false)?
            };
            anyhow::ensure!(
                script.is_file(),
                "native bundle not found: {}",
                script.display()
            );
            run_native(script, None, Some(manifest), root)
        }
        Some(Command::Diagnostics) => platform::print_diagnostics(),
        None => {
            let script = project_path(cli.script);
            let asset_root = runtime_root(&script);
            run_native(script, cli.asset.map(project_path), None, asset_root)
        }
    }
}
