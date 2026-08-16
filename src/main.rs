mod asset;
mod bridge;
mod js_runtime;
mod platform;
mod project;
mod renderer;

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, Parser, Subcommand};
use js_runtime::JsRuntime;
use project::Manifest;
use renderer::Renderer;
use std::{path::PathBuf, sync::Arc};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
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

fn run_native(
    script: PathBuf,
    asset_path: Option<PathBuf>,
    manifest: Option<Manifest>,
) -> Result<()> {
    let render_state = bridge::NativeRenderState::shared();
    let mut runtime = JsRuntime::new(render_state.clone())?;
    runtime.execute_source(include_str!("../js/three-bridge.js"))?;
    runtime.execute_file(&script)?;
    log::info!("executed JavaScript entry point: {}", script.display());
    let snapshot = render_state
        .lock()
        .expect("render state mutex should not be poisoned")
        .snapshot();
    log::info!(
        "native bridge state: clear={:?}, triangle_colors={:?}",
        snapshot.clear_color,
        snapshot.vertex_colors
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
    let mut renderer = pollster::block_on(Renderer::new(window, render_state))?;

    event_loop.run(move |event, event_loop| {
        event_loop.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { window_id, event } if window_id == renderer.window.id() => {
                match event {
                    WindowEvent::CloseRequested => event_loop.exit(),
                    WindowEvent::Resized(size) => renderer.resize(size),
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
            Event::AboutToWait => renderer.window.request_redraw(),
            _ => {}
        }
    })?;
    Ok(())
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
            run_native(script, None, Some(manifest))
        }
        Some(Command::Diagnostics) => platform::print_diagnostics(),
        None => run_native(project_path(cli.script), cli.asset.map(project_path), None),
    }
}
