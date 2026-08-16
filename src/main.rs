mod asset;
mod audio;
mod bridge;
mod draco;
mod js_runtime;
mod platform;
mod project;
mod renderer;
mod storage;
mod webgpu;

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, Parser, Subcommand};
use js_runtime::JsRuntime;
use project::Manifest;
use renderer::Renderer;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use winit::{
    event::{Event, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{ModifiersState, PhysicalKey},
    window::{CursorGrabMode, Fullscreen, WindowBuilder},
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

struct GameHost {
    renderer: Option<Renderer>,
    runtime: Option<JsRuntime>,
    script: PathBuf,
    asset_root: PathBuf,
    render_state: bridge::SharedRenderState,
    input_state: bridge::SharedInputState,
    transparent: bool,
    restart_count: u32,
    modifiers: ModifiersState,
}

impl GameHost {
    fn start_session(
        window: Arc<winit::window::Window>,
        script: &Path,
        asset_root: &Path,
        render_state: bridge::SharedRenderState,
        input_state: bridge::SharedInputState,
        transparent: bool,
        restart_count: u32,
    ) -> Result<(Renderer, JsRuntime)> {
        let renderer =
            pollster::block_on(Renderer::new(window, render_state.clone(), transparent))?;
        let mut runtime = JsRuntime::new_with_gpu(
            render_state,
            input_state,
            asset_root,
            Some(renderer.webgpu_context()),
        )?;
        runtime.execute_source(include_str!("../js/three-bridge.js"))?;
        runtime.execute_source(&format!(
            "globalThis.__hyperthreeNativeRestartCount={restart_count};"
        ))?;
        let initial_size = renderer.window.inner_size();
        runtime.set_window_size(
            initial_size.width,
            initial_size.height,
            renderer.window.scale_factor(),
        )?;
        runtime.execute_file(script)?;
        runtime.set_window_size(
            initial_size.width,
            initial_size.height,
            renderer.window.scale_factor(),
        )?;
        runtime.execute_start()?;
        log::info!("executed JavaScript entry point: {}", script.display());
        Ok((renderer, runtime))
    }

    fn new(
        window: Arc<winit::window::Window>,
        script: PathBuf,
        asset_root: PathBuf,
        render_state: bridge::SharedRenderState,
        input_state: bridge::SharedInputState,
        transparent: bool,
    ) -> Result<Self> {
        let (renderer, runtime) = Self::start_session(
            window,
            &script,
            &asset_root,
            render_state.clone(),
            input_state.clone(),
            transparent,
            0,
        )?;
        Ok(Self {
            renderer: Some(renderer),
            runtime: Some(runtime),
            script,
            asset_root,
            render_state,
            input_state,
            transparent,
            restart_count: 0,
            modifiers: ModifiersState::default(),
        })
    }

    fn renderer(&self) -> &Renderer {
        self.renderer
            .as_ref()
            .expect("game renderer is initialized")
    }

    fn renderer_mut(&mut self) -> &mut Renderer {
        self.renderer
            .as_mut()
            .expect("game renderer is initialized")
    }

    fn runtime_mut(&mut self) -> &mut JsRuntime {
        self.runtime.as_mut().expect("game runtime is initialized")
    }

    fn restart_after_device_loss(&mut self) -> Result<()> {
        if self.restart_count >= 3 {
            anyhow::bail!("native GPU device lost repeatedly during restart")
        }
        self.restart_count += 1;
        let window = self.renderer().window.clone();
        if let Some(runtime) = self.runtime.as_mut() {
            let _ = runtime.execute_shutdown();
        }
        drop(self.runtime.take());
        drop(self.renderer.take());
        let (renderer, runtime) = Self::start_session(
            window,
            &self.script,
            &self.asset_root,
            self.render_state.clone(),
            self.input_state.clone(),
            self.transparent,
            self.restart_count,
        )?;
        self.renderer = Some(renderer);
        self.runtime = Some(runtime);
        log::warn!("restarted native game session after device loss");
        Ok(())
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
            .with_transparent(
                manifest
                    .as_ref()
                    .map(|manifest| manifest.window.transparent)
                    .unwrap_or(false),
            )
            .build(&event_loop)
            .context("failed to create native window")?,
    );
    let mut host = GameHost::new(
        window,
        script,
        asset_root,
        render_state.clone(),
        input_state.clone(),
        manifest
            .as_ref()
            .map(|manifest| manifest.window.transparent)
            .unwrap_or(false),
    )?;
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
            Event::WindowEvent { window_id, event } if window_id == host.renderer().window.id() => {
                match event {
                    WindowEvent::CloseRequested => {
                        if let Err(error) = host.runtime_mut().execute_shutdown() {
                            log::error!("JavaScript shutdown callback failed: {error:#}");
                        }
                        event_loop.exit();
                    }
                    WindowEvent::Resized(size) => {
                        if host
                            .renderer()
                            .webgpu_context()
                            .device_lost_message()
                            .is_none()
                        {
                            host.renderer_mut().resize(size);
                            let scale_factor = host.renderer().window.scale_factor();
                            if let Err(error) = host.runtime_mut().set_window_size(
                                size.width,
                                size.height,
                                scale_factor,
                            ) {
                                log::error!("JavaScript resize event failed: {error:#}");
                            }
                        }
                    }
                    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        let size = host.renderer().window.inner_size();
                        if host
                            .renderer()
                            .webgpu_context()
                            .device_lost_message()
                            .is_none()
                        {
                            host.renderer_mut().resize(size);
                            if let Err(error) = host.runtime_mut().set_window_size(
                                size.width,
                                size.height,
                                scale_factor,
                            ) {
                                log::error!("JavaScript scale-factor resize failed: {error:#}");
                            }
                        }
                    }
                    WindowEvent::Focused(false) => {
                        input_state
                            .lock()
                            .expect("input state mutex should not be poisoned")
                            .clear();
                        host.modifiers = ModifiersState::default();
                        dispatch_input_event(&mut host, "blur", json!({}));
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        let physical_key = event.physical_key;
                        let code = format!("{physical_key:?}");
                        if let PhysicalKey::Code(code) = physical_key {
                            input_state
                                .lock()
                                .expect("input state mutex should not be poisoned")
                                .set_key(format!("{code:?}"), event.state.is_pressed());
                        }
                        let key = event
                            .logical_key
                            .to_text()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{:?}", event.logical_key));
                        let text = event.text.map(|text| text.to_string());
                        let event_type = if event.state.is_pressed() {
                            "keydown"
                        } else {
                            "keyup"
                        };
                        let modifiers = host.modifiers;
                        dispatch_input_event(
                            &mut host,
                            event_type,
                            json!({
                                "key": key,
                                "code": code,
                                "repeat": event.repeat,
                                "text": text,
                                "ctrlKey": modifiers.control_key(),
                                "shiftKey": modifiers.shift_key(),
                                "altKey": modifiers.alt_key(),
                                "metaKey": modifiers.super_key(),
                            }),
                        );
                    }
                    WindowEvent::ModifiersChanged(modifiers) => {
                        host.modifiers = modifiers.state();
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let (previous, buttons) = {
                            let mut input = input_state
                                .lock()
                                .expect("input state mutex should not be poisoned");
                            let previous = input.mouse_position();
                            input.set_mouse_position(position.x, position.y);
                            (previous, input.mouse_buttons_mask())
                        };
                        let init = json!({
                            "clientX": position.x,
                            "clientY": position.y,
                            "movementX": position.x - previous[0],
                            "movementY": position.y - previous[1],
                            "buttons": buttons,
                            "button": -1,
                            "pointerId": 1,
                            "pointerType": "mouse",
                            "isPrimary": true,
                            "ctrlKey": host.modifiers.control_key(),
                            "shiftKey": host.modifiers.shift_key(),
                            "altKey": host.modifiers.alt_key(),
                            "metaKey": host.modifiers.super_key(),
                        });
                        dispatch_input_event(&mut host, "mousemove", init.clone());
                        dispatch_input_event(&mut host, "pointermove", init);
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if let Some(button) = mouse_button_id(button) {
                            let (position, buttons) = {
                                let mut input = input_state
                                    .lock()
                                    .expect("input state mutex should not be poisoned");
                                input.set_mouse_button(button, state.is_pressed());
                                (input.mouse_position(), input.mouse_buttons_mask())
                            };
                            let init = json!({
                                "clientX": position[0],
                                "clientY": position[1],
                                "button": button,
                                "buttons": buttons,
                                "pointerId": 1,
                                "pointerType": "mouse",
                                "isPrimary": true,
                                "ctrlKey": host.modifiers.control_key(),
                                "shiftKey": host.modifiers.shift_key(),
                                "altKey": host.modifiers.alt_key(),
                                "metaKey": host.modifiers.super_key(),
                            });
                            let event_type = if state.is_pressed() {
                                "mousedown"
                            } else {
                                "mouseup"
                            };
                            let pointer_event_type = if state.is_pressed() {
                                "pointerdown"
                            } else {
                                "pointerup"
                            };
                            dispatch_input_event(&mut host, event_type, init.clone());
                            dispatch_input_event(&mut host, pointer_event_type, init.clone());
                            if !state.is_pressed() && button == 0 {
                                dispatch_input_event(&mut host, "click", init);
                            }
                        }
                    }
                    WindowEvent::CursorEntered { .. } => {
                        dispatch_input_event(&mut host, "mouseenter", json!({}));
                        dispatch_input_event(
                            &mut host,
                            "pointerenter",
                            json!({
                                "pointerId": 1,
                                "pointerType": "mouse",
                                "isPrimary": true,
                            }),
                        );
                    }
                    WindowEvent::CursorLeft { .. } => {
                        dispatch_input_event(&mut host, "mouseleave", json!({}));
                        dispatch_input_event(
                            &mut host,
                            "pointerleave",
                            json!({
                                "pointerId": 1,
                                "pointerType": "mouse",
                                "isPrimary": true,
                            }),
                        );
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        let (delta_x, delta_y, delta_mode) = match delta {
                            MouseScrollDelta::LineDelta(x, y) => (x as f64, y as f64, 1),
                            MouseScrollDelta::PixelDelta(delta) => (delta.x, delta.y, 0),
                        };
                        let position = input_state
                            .lock()
                            .expect("input state mutex should not be poisoned")
                            .mouse_position();
                        let modifiers = host.modifiers;
                        dispatch_input_event(
                            &mut host,
                            "wheel",
                            json!({
                                "clientX": position[0],
                                "clientY": position[1],
                                "deltaX": delta_x,
                                "deltaY": delta_y,
                                "deltaZ": 0,
                                "deltaMode": delta_mode,
                                "ctrlKey": modifiers.control_key(),
                                "shiftKey": modifiers.shift_key(),
                                "altKey": modifiers.alt_key(),
                                "metaKey": modifiers.super_key(),
                            }),
                        );
                    }
                    WindowEvent::Touch(touch) => {
                        let (event_type, pointer_event_type) = match touch.phase {
                            TouchPhase::Started => ("touchstart", "pointerdown"),
                            TouchPhase::Moved => ("touchmove", "pointermove"),
                            TouchPhase::Ended => ("touchend", "pointerup"),
                            TouchPhase::Cancelled => ("touchcancel", "pointercancel"),
                        };
                        let init = json!({
                            "clientX": touch.location.x,
                            "clientY": touch.location.y,
                            "pageX": touch.location.x,
                            "pageY": touch.location.y,
                            "pointerId": touch.id,
                            "pointerType": "touch",
                            "isPrimary": true,
                            "pressure": touch.force.map(|_| 1.0).unwrap_or(0.5),
                        });
                        dispatch_input_event(&mut host, event_type, init.clone());
                        dispatch_input_event(&mut host, pointer_event_type, init);
                    }
                    WindowEvent::RedrawRequested => {
                        if host
                            .renderer()
                            .webgpu_context()
                            .device_lost_message()
                            .is_none()
                        {
                            match host.renderer_mut().render() {
                                Ok(()) => {}
                                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                    let size = host.renderer().window.inner_size();
                                    if host
                                        .renderer()
                                        .webgpu_context()
                                        .device_lost_message()
                                        .is_none()
                                    {
                                        host.renderer_mut().resize(size)
                                    }
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                                Err(wgpu::SurfaceError::Timeout) => {
                                    log::warn!("surface timeout; skipping frame")
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                if let Some(lock) = input_state
                    .lock()
                    .expect("input state mutex should not be poisoned")
                    .take_pointer_lock_request()
                {
                    apply_pointer_lock_request(&mut host, lock);
                }
                if let Some(fullscreen) = input_state
                    .lock()
                    .expect("input state mutex should not be poisoned")
                    .take_fullscreen_request()
                {
                    apply_fullscreen_request(&mut host, fullscreen);
                }
                let now = Instant::now();
                let delta_seconds = now.duration_since(last_frame).as_secs_f64().clamp(0.0, 0.1);
                last_frame = now;
                if let Err(error) = host.runtime_mut().execute_frame(delta_seconds) {
                    log::error!("JavaScript frame update failed: {error:#}");
                    if let Err(shutdown_error) = host.runtime_mut().execute_shutdown() {
                        log::error!("JavaScript shutdown callback failed: {shutdown_error:#}");
                    }
                    event_loop.exit();
                } else if let Some(message) = host.renderer().webgpu_context().device_lost_message()
                {
                    log::warn!("native GPU device lost; restarting game session: {message}");
                    if let Err(error) = host.restart_after_device_loss() {
                        log::error!("native GPU session restart failed: {error:#}");
                        event_loop.exit();
                    } else {
                        last_frame = Instant::now();
                        host.renderer().window.request_redraw();
                    }
                } else {
                    host.renderer().window.request_redraw();
                }
            }
            _ => {}
        }
    })?;
    Ok(())
}

fn dispatch_input_event(host: &mut GameHost, event_type: &str, init: serde_json::Value) {
    if let Err(error) = host.runtime_mut().dispatch_input_event(event_type, &init) {
        log::error!("JavaScript {event_type} event failed: {error:#}");
    }
}

fn apply_pointer_lock_request(host: &mut GameHost, lock: bool) {
    let window = host.renderer().window.clone();
    let locked = if lock {
        let result = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        if let Err(error) = result {
            log::warn!("native pointer lock request failed: {error}");
            false
        } else {
            window.set_cursor_visible(false);
            true
        }
    } else {
        let result = window.set_cursor_grab(CursorGrabMode::None);
        if let Err(error) = result {
            log::debug!("native pointer unlock request failed: {error}");
        }
        window.set_cursor_visible(true);
        false
    };
    host.input_state
        .lock()
        .expect("input state mutex should not be poisoned")
        .set_pointer_locked(locked);
    dispatch_input_event(host, "pointerlockchange", json!({}));
}

fn apply_fullscreen_request(host: &mut GameHost, fullscreen: bool) {
    let window = host.renderer().window.clone();
    if fullscreen {
        window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
    } else {
        window.set_fullscreen(None);
    }
    host.input_state
        .lock()
        .expect("input state mutex should not be poisoned")
        .set_fullscreen(fullscreen);
    dispatch_input_event(host, "fullscreenchange", json!({}));
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
