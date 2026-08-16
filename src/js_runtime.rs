use crate::{
    asset::AssetStore,
    bridge::{SharedInputState, SharedRenderState},
};
use anyhow::{Context as _, Result};
use boa_engine::{
    builtins::promise::PromiseState,
    js_string,
    module::{Module, ModuleLoader, Referrer},
    object::JsObject,
    Context, JsArgs, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source,
};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};

/// JavaScript execution boundary.
///
/// This first vertical slice uses an embeddable ECMAScript runtime so the
/// host can execute the game entry point without a browser. The public shape
/// intentionally stays small: the runtime can be replaced by Embedded V8
/// without changing the window, asset, or renderer layers.
pub struct JsRuntime {
    context: Context,
}

impl JsRuntime {
    pub fn new(
        render_state: SharedRenderState,
        input_state: SharedInputState,
        asset_root: impl AsRef<Path>,
    ) -> Result<Self> {
        let asset_root = asset_root.as_ref().to_path_buf();
        let asset_store = Arc::new(Mutex::new(AssetStore::new(&asset_root)?));
        let module_loader = Rc::new(ProjectModuleLoader::new(&asset_root)?);
        let mut context = Context::builder()
            .module_loader(module_loader)
            .build()
            .map_err(|error| anyhow::anyhow!("failed to create JavaScript context: {error}"))?;
        let runtime_start = Instant::now();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeNow"), 0, unsafe {
                NativeFunction::from_closure(move |_this, _args, _context| {
                    Ok(JsValue::from(
                        runtime_start.elapsed().as_secs_f64() * 1000.0,
                    ))
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to register runtime clock binding: {error}")
            })?;
        let clear_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreeSetClearColor"),
                4,
                // The closure captures only Arc<Mutex<...>> Rust state, never a
                // GC-managed Boa value, so it does not need GC tracing.
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let color = [
                            number_arg(args, 0, context)?,
                            number_arg(args, 1, context)?,
                            number_arg(args, 2, context)?,
                            number_arg(args, 3, context)?,
                        ];
                        clear_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .set_clear_color(color);
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| anyhow::anyhow!("failed to register clear-color binding: {error}"))?;

        let cube_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeSetCube"), 12, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let position = [
                        number_arg(args, 0, context)?,
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                    ];
                    let scale = [
                        number_arg(args, 3, context)?,
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                    ];
                    let rotation_y = number_arg(args, 6, context)?;
                    let color = [
                        number_arg(args, 7, context)?,
                        number_arg(args, 8, context)?,
                        number_arg(args, 9, context)?,
                        number_arg(args, 10, context)?,
                    ];
                    let _reserved = number_arg(args, 11, context)?;
                    cube_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .set_cube(position, scale, rotation_y, color);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register cube binding: {error}"))?;

        let frame_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeBeginFrame"), 0, unsafe {
                NativeFunction::from_closure(move |_this, _args, _context| {
                    frame_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .begin_frame();
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register frame binding: {error}"))?;

        let instance_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreePushCube"), 12, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let position = [
                        number_arg(args, 0, context)?,
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                    ];
                    let scale = [
                        number_arg(args, 3, context)?,
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                    ];
                    let rotation_y = number_arg(args, 6, context)?;
                    let color = [
                        number_arg(args, 7, context)?,
                        number_arg(args, 8, context)?,
                        number_arg(args, 9, context)?,
                        number_arg(args, 10, context)?,
                    ];
                    let _reserved = number_arg(args, 11, context)?;
                    instance_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .push_cube(position, scale, rotation_y, color);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register instance binding: {error}"))?;

        let plane_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreePushPlane"), 12, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let position = [
                        number_arg(args, 0, context)?,
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                    ];
                    let scale = [
                        number_arg(args, 3, context)?,
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                    ];
                    let rotation_y = number_arg(args, 6, context)?;
                    let color = [
                        number_arg(args, 7, context)?,
                        number_arg(args, 8, context)?,
                        number_arg(args, 9, context)?,
                        number_arg(args, 10, context)?,
                    ];
                    let _reserved = number_arg(args, 11, context)?;
                    plane_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .push_plane(position, scale, rotation_y, color);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to register plane instance binding: {error}")
            })?;

        let sphere_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreePushSphere"), 12, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let position = [
                        number_arg(args, 0, context)?,
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                    ];
                    let scale = [
                        number_arg(args, 3, context)?,
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                    ];
                    let rotation_y = number_arg(args, 6, context)?;
                    let color = [
                        number_arg(args, 7, context)?,
                        number_arg(args, 8, context)?,
                        number_arg(args, 9, context)?,
                        number_arg(args, 10, context)?,
                    ];
                    let _reserved = number_arg(args, 11, context)?;
                    sphere_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .push_sphere(position, scale, rotation_y, color);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to register sphere instance binding: {error}")
            })?;

        let geometry_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreeRegisterGeometry"),
                3,
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let geometry_id = geometry_id_arg(args, 0, context)?;
                        let position_values = number_array_arg(args, 1, context)?;
                        if position_values.len() % 3 != 0 {
                            return Err(JsNativeError::range()
                                .with_message("position attribute length must be divisible by 3")
                                .into());
                        }
                        let positions = position_values
                            .chunks_exact(3)
                            .map(|position| {
                                [position[0] as f32, position[1] as f32, position[2] as f32]
                            })
                            .collect::<Vec<_>>();
                        let index_values = number_array_arg(args, 2, context)?;
                        let indices = if index_values.is_empty() {
                            (0..positions.len() as u32).collect::<Vec<_>>()
                        } else {
                            index_values
                                .into_iter()
                                .map(|index| {
                                    if index < 0.0 || index.fract() != 0.0 {
                                        return Err(JsNativeError::range()
                                            .with_message(
                                                "geometry indices must be non-negative integers",
                                            )
                                            .into());
                                    }
                                    Ok(index as u32)
                                })
                                .collect::<JsResult<Vec<_>>>()?
                        };
                        geometry_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .register_geometry(geometry_id, positions, indices)
                            .map_err(|error| JsNativeError::range().with_message(error))?;
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| anyhow::anyhow!("failed to register geometry binding: {error}"))?;

        let geometry_instance_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreePushGeometry"), 12, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let geometry_id = geometry_id_arg(args, 0, context)?;
                    let position = [
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                        number_arg(args, 3, context)?,
                    ];
                    let scale = [
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                        number_arg(args, 6, context)?,
                    ];
                    let rotation_y = number_arg(args, 7, context)?;
                    let color = [
                        number_arg(args, 8, context)?,
                        number_arg(args, 9, context)?,
                        number_arg(args, 10, context)?,
                        number_arg(args, 11, context)?,
                    ];
                    geometry_instance_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .push_custom_mesh(geometry_id, position, scale, rotation_y, color);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to register geometry instance binding: {error}")
            })?;

        let camera_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeSetCamera"), 9, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let position = [
                        number_arg(args, 0, context)?,
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                    ];
                    let target = [
                        number_arg(args, 3, context)?,
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                    ];
                    let fov_y = number_arg(args, 6, context)?;
                    let near = number_arg(args, 7, context)?;
                    let far = number_arg(args, 8, context)?;
                    camera_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .set_camera(position, target, fov_y, near, far);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register camera binding: {error}"))?;

        let orthographic_camera_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreeSetOrthographicCamera"),
                12,
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let position = [
                            number_arg(args, 0, context)?,
                            number_arg(args, 1, context)?,
                            number_arg(args, 2, context)?,
                        ];
                        let target = [
                            number_arg(args, 3, context)?,
                            number_arg(args, 4, context)?,
                            number_arg(args, 5, context)?,
                        ];
                        let left = number_arg(args, 6, context)?;
                        let right = number_arg(args, 7, context)?;
                        let top = number_arg(args, 8, context)?;
                        let bottom = number_arg(args, 9, context)?;
                        let near = number_arg(args, 10, context)?;
                        let far = number_arg(args, 11, context)?;
                        orthographic_camera_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .set_orthographic_camera(
                                position,
                                target,
                                [left, right, top, bottom],
                                near,
                                far,
                            );
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| {
                anyhow::anyhow!("failed to register orthographic camera binding: {error}")
            })?;

        let key_input_state = input_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeIsKeyDown"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let code = args
                        .get_or_undefined(0)
                        .to_string(context)
                        .map_err(|_| {
                            JsNativeError::typ().with_message("key code must be a string")
                        })?
                        .to_std_string_escaped();
                    let pressed = key_input_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("input state poisoned"))?
                        .is_key_down(&code);
                    Ok(JsValue::from(pressed))
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register keyboard binding: {error}"))?;

        let mouse_button_input_state = input_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreeIsMouseButtonDown"),
                1,
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let button = nonnegative_usize_arg(args, 0, context)?;
                        let pressed = mouse_button_input_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("input state poisoned")
                            })?
                            .is_mouse_button_down(button as u8);
                        Ok(JsValue::from(pressed))
                    })
                },
            )
            .map_err(|error| anyhow::anyhow!("failed to register mouse button binding: {error}"))?;

        let mouse_position_input_state = input_state;
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreeGetMousePosition"),
                0,
                unsafe {
                    NativeFunction::from_closure(move |_this, _args, context| {
                        let position = mouse_position_input_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("input state poisoned")
                            })?
                            .mouse_position();
                        let value = JsObject::with_object_proto(context.intrinsics());
                        value.set(js_string!("x"), JsValue::from(position[0]), false, context)?;
                        value.set(js_string!("y"), JsValue::from(position[1]), false, context)?;
                        Ok(value.into())
                    })
                },
            )
            .map_err(|error| {
                anyhow::anyhow!("failed to register mouse position binding: {error}")
            })?;

        let asset_store_for_load = asset_store.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeLoadAsset"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let path = string_arg(args, 0, context)?;
                    let metadata = asset_store_for_load
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("asset store poisoned"))?
                        .load(&path)
                        .map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
                    let asset = JsObject::with_object_proto(context.intrinsics());
                    asset.set(
                        js_string!("path"),
                        js_string!(metadata.relative_path),
                        false,
                        context,
                    )?;
                    asset.set(
                        js_string!("byteLength"),
                        JsValue::from(metadata.byte_length as f64),
                        false,
                        context,
                    )?;
                    asset.set(
                        js_string!("format"),
                        js_string!(metadata.format),
                        false,
                        context,
                    )?;
                    asset.set(
                        js_string!("meshCount"),
                        JsValue::from(metadata.mesh_count as f64),
                        false,
                        context,
                    )?;
                    asset.set(
                        js_string!("primitiveCount"),
                        JsValue::from(metadata.primitive_count as f64),
                        false,
                        context,
                    )?;
                    asset.set(
                        js_string!("animationCount"),
                        JsValue::from(metadata.animation_count as f64),
                        false,
                        context,
                    )?;
                    Ok(asset.into())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register asset binding: {error}"))?;

        let asset_draw_store = asset_store;
        let asset_draw_state = render_state;
        context
            .register_global_builtin_callable(js_string!("__hyperthreeDrawAsset"), 14, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let path = string_arg(args, 0, context)?;
                    let mesh_index = nonnegative_usize_arg(args, 1, context)?;
                    let primitive_index = nonnegative_usize_arg(args, 2, context)?;
                    let position = [
                        number_arg(args, 3, context)?,
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                    ];
                    let scale = [
                        number_arg(args, 6, context)?,
                        number_arg(args, 7, context)?,
                        number_arg(args, 8, context)?,
                    ];
                    let rotation_y = number_arg(args, 9, context)?;
                    let color = [
                        number_arg(args, 10, context)?,
                        number_arg(args, 11, context)?,
                        number_arg(args, 12, context)?,
                        number_arg(args, 13, context)?,
                    ];
                    let geometry = asset_draw_store
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("asset store poisoned"))?
                        .load_geometry(&path, mesh_index, primitive_index)
                        .map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
                    let mut state = asset_draw_state.lock().map_err(|_| {
                        JsNativeError::error().with_message("render state poisoned")
                    })?;
                    state
                        .register_geometry(
                            geometry.geometry_id,
                            geometry.positions.clone(),
                            geometry.indices.clone(),
                        )
                        .map_err(|error| JsNativeError::range().with_message(error))?;
                    state.push_custom_mesh(
                        geometry.geometry_id,
                        position,
                        scale,
                        rotation_y,
                        color,
                    );
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register asset draw binding: {error}"))?;

        context
            .eval(Source::from_bytes(
                r#"
                globalThis.performance = globalThis.performance || {
                  now: () => __hyperthreeNow(),
                };
                globalThis.window = globalThis.window || globalThis;
                globalThis.self = globalThis.self || globalThis;
                globalThis.global = globalThis.global || globalThis;
                globalThis.__hyperthreeAnimationFrameQueue = [];
                globalThis.__hyperthreeAnimationFrameId = 0;
                globalThis.requestAnimationFrame = (callback) => {
                  const id = ++globalThis.__hyperthreeAnimationFrameId;
                  globalThis.__hyperthreeAnimationFrameQueue.push({ id, callback });
                  return id;
                };
                globalThis.cancelAnimationFrame = (id) => {
                  globalThis.__hyperthreeAnimationFrameQueue =
                    globalThis.__hyperthreeAnimationFrameQueue.filter((entry) => entry.id !== id);
                };
                "#,
            ))
            .map(|_| ())
            .map_err(|error| {
                anyhow::anyhow!("failed to install runtime compatibility globals: {error}")
            })?;

        Ok(Self { context })
    }

    pub fn execute_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read JavaScript entry point {}", path.display()))?;
        if is_module_source(path, &source) {
            self.execute_module(path, &source)
        } else {
            self.execute_source(&source)
        }
    }

    pub fn execute_source(&mut self, source: &str) -> Result<()> {
        self.context
            .eval(Source::from_bytes(source))
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("JavaScript evaluation failed: {error}"))
    }

    fn execute_module(&mut self, path: &Path, source: &str) -> Result<()> {
        let module = Module::parse(
            Source::from_bytes(source).with_path(path),
            None,
            &mut self.context,
        )
        .map_err(|error| anyhow::anyhow!("JavaScript module parse failed: {error}"))?;
        let promise = module.load_link_evaluate(&mut self.context);
        self.context
            .run_jobs()
            .map_err(|error| anyhow::anyhow!("JavaScript module jobs failed: {error}"))?;
        match promise.state() {
            PromiseState::Fulfilled(_) => Ok(()),
            PromiseState::Pending => Err(anyhow::anyhow!(
                "JavaScript module evaluation did not settle"
            )),
            PromiseState::Rejected(reason) => Err(anyhow::anyhow!(
                "JavaScript module evaluation failed: {}",
                reason.display()
            )),
        }
    }

    pub fn execute_frame(&mut self, delta_seconds: f64) -> Result<()> {
        let source = format!(
            "(() => {{ const pendingFrames = globalThis.__hyperthreeAnimationFrameQueue || []; globalThis.__hyperthreeAnimationFrameQueue = []; pendingFrames.forEach((entry) => {{ if (typeof entry.callback === 'function') entry.callback(performance.now()); }}); }})(); if (typeof globalThis.HyperThreeGame !== 'undefined' && typeof globalThis.HyperThreeGame.update === 'function') globalThis.HyperThreeGame.update({delta_seconds});"
        );
        self.execute_source(&source)
    }

    pub fn execute_start(&mut self) -> Result<()> {
        self.execute_lifecycle_callback("onStart")
    }

    pub fn execute_shutdown(&mut self) -> Result<()> {
        self.execute_lifecycle_callback("onStop")
    }

    fn execute_lifecycle_callback(&mut self, callback: &str) -> Result<()> {
        let source = format!(
            "if (typeof globalThis.HyperThreeGame !== 'undefined' && typeof globalThis.HyperThreeGame.{callback} === 'function') globalThis.HyperThreeGame.{callback}();"
        );
        self.execute_source(&source)
    }
}

fn is_module_source(path: &Path, source: &str) -> bool {
    path.extension().is_some_and(|extension| extension == "mjs")
        || source.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("import ")
                || line.starts_with("import{")
                || line.starts_with("export ")
                || line.starts_with("export{")
        })
}

struct ProjectModuleLoader {
    root: PathBuf,
}

impl ProjectModuleLoader {
    fn new(root: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            root: root.as_ref().canonicalize().with_context(|| {
                format!("module root does not exist: {}", root.as_ref().display())
            })?,
        })
    }

    fn resolve_path(&self, referrer: &Referrer, specifier: &JsString) -> JsResult<PathBuf> {
        let specifier_text = specifier.to_std_string_escaped();
        let short_path = Path::new(&specifier_text);
        let path = if short_path.starts_with(".") {
            let referrer_path = referrer.path().ok_or_else(|| {
                JsError::from_opaque(js_string!("relative import without a referrer").into())
            })?;
            referrer_path
                .parent()
                .unwrap_or(&self.root)
                .join(short_path)
        } else if short_path.is_absolute() {
            PathBuf::from(short_path)
        } else {
            self.resolve_package(&specifier_text).map_err(|error| {
                JsError::from(
                    JsNativeError::typ()
                        .with_message(error.to_string())
                        .with_cause(JsError::from_opaque(js_string!(specifier_text).into())),
                )
            })?
        };
        let path = resolve_existing_module_path(path)
            .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
        let canonical = path
            .canonicalize()
            .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
        if !canonical.starts_with(&self.root) {
            return Err(JsNativeError::typ()
                .with_message("module path is outside the project root")
                .into());
        }
        Ok(canonical)
    }

    fn resolve_package(&self, specifier: &str) -> Result<PathBuf> {
        let mut parts = specifier.split('/');
        let package = match parts.next() {
            Some(scope) if scope.starts_with('@') => {
                let name = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("invalid package specifier: {specifier}"))?;
                format!("{scope}/{name}")
            }
            Some(name) => name.to_string(),
            None => anyhow::bail!("empty package specifier"),
        };
        let package_root = self.root.join("node_modules").join(&package);
        let subpath = specifier.strip_prefix(&package).unwrap_or_default();
        let entry = if subpath.is_empty() {
            let package_source = fs::read_to_string(package_root.join("package.json"))
                .with_context(|| format!("package `{package}` has no readable package.json"))?;
            let package_json: serde_json::Value = serde_json::from_str(&package_source)?;
            package_entry(&package_json, ".")
                .or_else(|| {
                    package_json
                        .get("module")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .or_else(|| {
                    package_json
                        .get("main")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("index.js"))
        } else {
            let package_source = fs::read_to_string(package_root.join("package.json")).ok();
            let package_json = package_source
                .as_deref()
                .and_then(|source| serde_json::from_str::<serde_json::Value>(source).ok());
            package_json
                .as_ref()
                .and_then(|json| {
                    package_entry(json, &format!("./{}", subpath.trim_start_matches('/')))
                })
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(subpath.trim_start_matches('/')))
        };
        Ok(package_root.join(entry))
    }
}

fn package_entry(package_json: &serde_json::Value, condition: &str) -> Option<String> {
    let exports = package_json.get("exports")?;
    let target = if exports.is_object() && exports.get(condition).is_some() {
        exports.get(condition)?
    } else {
        exports
    };
    export_target(target).map(str::to_string)
}

fn export_target(value: &serde_json::Value) -> Option<&str> {
    if let Some(target) = value.as_str() {
        return Some(target);
    }
    let object = value.as_object()?;
    object
        .get("import")
        .and_then(export_target)
        .or_else(|| object.get("default").and_then(export_target))
}

impl ModuleLoader for ProjectModuleLoader {
    fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> impl std::future::Future<Output = JsResult<Module>> {
        let result = (|| {
            let path = self.resolve_path(&referrer, &specifier)?;
            let source = Source::from_filepath(&path).map_err(|error| {
                JsNativeError::typ()
                    .with_message(format!("could not open module `{}`", path.display()))
                    .with_cause(JsError::from_opaque(js_string!(error.to_string()).into()))
            })?;
            Module::parse(source, None, &mut context.borrow_mut()).map_err(|error| {
                JsNativeError::syntax()
                    .with_message(format!("could not parse module `{}`", path.display()))
                    .with_cause(error)
                    .into()
            })
        })();
        async move { result }
    }
}

fn resolve_existing_module_path(path: PathBuf) -> Result<PathBuf> {
    let candidates = if path.extension().is_some() {
        vec![path.clone()]
    } else {
        vec![
            path.with_extension("js"),
            path.with_extension("mjs"),
            path.join("index.js"),
        ]
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| anyhow::anyhow!("module does not exist: {}", path.display()))
}

fn string_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    args.get_or_undefined(index)
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .map_err(|_| {
            JsNativeError::typ()
                .with_message("asset path must be a string")
                .into()
        })
}

fn number_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<f64> {
    let value = args.get_or_undefined(index).to_number(context)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(JsNativeError::range()
            .with_message("native render values must be finite numbers")
            .into())
    }
}

fn geometry_id_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<u64> {
    let value = number_arg(args, index, context)?;
    if value < 0.0 || value.fract() != 0.0 || value > u64::MAX as f64 {
        return Err(JsNativeError::range()
            .with_message("geometry id must be a non-negative integer")
            .into());
    }
    Ok(value as u64)
}

fn nonnegative_usize_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<usize> {
    let value = number_arg(args, index, context)?;
    if value < 0.0 || value.fract() != 0.0 || value > usize::MAX as f64 {
        return Err(JsNativeError::range()
            .with_message("asset index must be a non-negative integer")
            .into());
    }
    Ok(value as usize)
}

fn number_array_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<Vec<f64>> {
    let object = args
        .get_or_undefined(index)
        .to_object(context)
        .map_err(|_| JsNativeError::typ().with_message("geometry attribute must be array-like"))?;
    let length = object
        .get(js_string!("length"), context)?
        .to_length(context)
        .map_err(|_| JsNativeError::typ().with_message("geometry attribute length is invalid"))?;
    if length > 3_000_000 {
        return Err(JsNativeError::range()
            .with_message("geometry attribute is too large")
            .into());
    }
    let mut values = Vec::with_capacity(length as usize);
    for index in 0..length as usize {
        let value = object.get(index, context)?.to_number(context)?;
        if !value.is_finite() {
            return Err(JsNativeError::range()
                .with_message("geometry attribute values must be finite numbers")
                .into());
        }
        values.push(value);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::JsRuntime;
    use crate::bridge::{NativeInputState, NativeRenderState};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn frame_callback_updates_native_state() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.HyperThreeGame = {
                  update(deltaSeconds) {
                    __hyperthreeBeginFrame();
                    __hyperthreePushCube(deltaSeconds, 0, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0);
                  }
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(0.25).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.cubes.len(), 1);
        assert_eq!(snapshot.cubes[0].position[0], 0.25);
    }

    #[test]
    fn javascript_reads_native_mouse_input() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        {
            let mut input = input_state.lock().unwrap();
            input.set_mouse_position(12.0, 24.0);
            input.set_mouse_button(0, true);
        }
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(include_str!("../js/three-bridge.js"))
            .unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.HyperThreeGame = {
                  update() {
                    const mouse = HyperThreeNative.getMousePosition();
                    __hyperthreeBeginFrame();
                    __hyperthreePushCube(mouse.x, mouse.y, 0, 1, 1, 1, 0,
                      HyperThreeNative.isMouseButtonDown(0) ? 1 : 0, 0, 0, 1, 0);
                  }
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(0.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.cubes[0].position, [12.0, 24.0, 0.0]);
        assert_eq!(snapshot.cubes[0].color[0], 1.0);
    }

    #[test]
    fn runtime_compatibility_globals_drive_animation_frames() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                requestAnimationFrame((timestamp) => {
                  __hyperthreeBeginFrame();
                  __hyperthreePushCube(timestamp >= 0 ? 1 : 0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0);
                });
                "#,
            )
            .unwrap();
        runtime.execute_frame(1.0 / 60.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.cubes[0].position[0], 1.0);
        runtime.execute_frame(1.0 / 60.0).unwrap();
    }

    #[test]
    fn three_scene_sync_converts_box_meshes_to_native_instances() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(include_str!("../js/three-bridge.js"))
            .unwrap();
        runtime
            .execute_source(
                r#"
                const scene = {
                  updateMatrixWorld() {},
                  traverse(callback) {
                    callback({
                      visible: true,
                      isMesh: true,
                      geometry: { type: "BoxGeometry" },
                      matrixWorld: { elements: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1, 2, 3, 1] },
                      scale: { x: 2, y: 3, z: 4 },
                      rotation: { y: 0.5 },
                      material: { color: { r: 0.2, g: 0.4, b: 0.6 }, opacity: 0.75 },
                    });
                  },
                };
                const camera = {
                  position: { x: 0, y: 0, z: 4 },
                  matrixWorld: { elements: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 4, 1] },
                  fov: 60,
                  near: 0.1,
                  far: 100,
                  updateMatrixWorld() {},
                };
                globalThis.HyperThreeGame = {
                  update() { globalThis.syncResult = HyperThreeNative.syncThreeScene(scene, camera); },
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(1.0 / 60.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.cubes.len(), 1);
        assert_eq!(snapshot.cubes[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(snapshot.cubes[0].scale, [2.0, 3.0, 4.0]);
        assert_eq!(snapshot.camera.target, [0.0, 0.0, 3.0]);
    }

    #[test]
    fn three_scene_sync_supports_orthographic_cameras() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(include_str!("../js/three-bridge.js"))
            .unwrap();
        runtime
            .execute_source(
                r#"
                const scene = { updateMatrixWorld() {}, traverse() {} };
                const camera = {
                  type: "OrthographicCamera",
                  isOrthographicCamera: true,
                  position: { x: 0, y: 0, z: 4 },
                  matrixWorld: { elements: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 4, 1] },
                  left: -4, right: 4, top: 3, bottom: -3, near: 0.1, far: 100,
                  updateMatrixWorld() {},
                };
                globalThis.HyperThreeGame = {
                  update() { HyperThreeNative.syncThreeScene(scene, camera); },
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(1.0 / 60.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert!(matches!(
            snapshot.camera.projection,
            crate::bridge::CameraProjection::Orthographic { .. }
        ));
    }

    #[test]
    fn three_scene_sync_routes_plane_and_sphere_primitives() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(include_str!("../js/three-bridge.js"))
            .unwrap();
        runtime
            .execute_source(
                r#"
                const objects = ["PlaneGeometry", "SphereGeometry"].map((type, index) => ({
                  visible: true,
                  isMesh: true,
                  geometry: { type },
                  position: { x: index, y: 0, z: 0 },
                  scale: { x: 1, y: 1, z: 1 },
                  rotation: { y: 0 },
                  material: { color: { r: 1, g: 1, b: 1 }, opacity: 1 },
                }));
                const scene = {
                  updateMatrixWorld() {},
                  traverse(callback) { objects.forEach(callback); },
                };
                globalThis.HyperThreeGame = {
                  update() { HyperThreeNative.syncThreeScene(scene); },
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(1.0 / 60.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.cubes.len(), 2);
        assert!(matches!(
            snapshot.cubes[0].geometry,
            crate::bridge::GeometryKind::Plane
        ));
        assert!(matches!(
            snapshot.cubes[1].geometry,
            crate::bridge::GeometryKind::Sphere
        ));
    }

    #[test]
    fn three_scene_sync_registers_and_reuses_buffer_geometry() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(include_str!("../js/three-bridge.js"))
            .unwrap();
        runtime
            .execute_source(
                r#"
                const geometry = {
                  id: 42,
                  type: "BufferGeometry",
                  attributes: { position: { array: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]) } },
                  index: { array: new Uint16Array([0, 1, 2]) },
                };
                const scene = {
                  updateMatrixWorld() {},
                  traverse(callback) {
                    callback({
                      visible: true,
                      isMesh: true,
                      geometry,
                      position: { x: 1, y: 2, z: 3 },
                      scale: { x: 1, y: 1, z: 1 },
                      rotation: { y: 0 },
                      material: { color: { r: 0.4, g: 0.5, b: 0.6 }, opacity: 1 },
                    });
                  },
                };
                globalThis.HyperThreeGame = {
                  update() { HyperThreeNative.syncThreeScene(scene); },
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(1.0 / 60.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.custom_meshes.len(), 1);
        assert_eq!(snapshot.custom_meshes[0].geometry_id, 42);
        assert_eq!(snapshot.custom_meshes[0].position, [1.0, 2.0, 3.0]);
        let registry = snapshot.geometry_registry.lock().unwrap();
        let geometry = registry.get(42).unwrap();
        assert_eq!(geometry.positions.len(), 3);
        assert_eq!(geometry.indices, [0, 1, 2]);
    }

    #[test]
    fn javascript_can_load_project_relative_asset_metadata() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.HyperThreeGame = {
                  update() {
                    const asset = __hyperthreeLoadAsset("Cargo.toml");
                    __hyperthreeBeginFrame();
                    __hyperthreePushCube(asset.byteLength, asset.meshCount, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0);
                  }
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(0.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert!(snapshot.cubes[0].position[0] > 0.0);
        assert_eq!(snapshot.cubes[0].position[1], 0.0);
    }

    #[test]
    fn javascript_can_decode_and_draw_gltf_geometry_without_js_buffers() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-gltf-draw-test-{suffix}"));
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(
            root.join("public/scene.gltf"),
            br#"{
              "asset": {"version": "2.0"},
              "buffers": [{"byteLength": 36, "uri": "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}],
              "bufferViews": [{"buffer": 0, "byteLength": 36}],
              "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 1]}],
              "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}]
            }"#,
        )
        .unwrap();
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, &root).unwrap();
        runtime
            .execute_source(include_str!("../js/three-bridge.js"))
            .unwrap();
        runtime
            .execute_source(
                r#"
                HyperThreeNative.drawAsset("public/scene.gltf", 0, 0, {
                  x: 4, y: 5, z: 6, r: 0.7, g: 0.6, b: 0.5,
                });
                "#,
            )
            .unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.custom_meshes.len(), 1);
        assert_eq!(snapshot.custom_meshes[0].position, [4.0, 5.0, 6.0]);
        assert!(snapshot
            .geometry_registry
            .lock()
            .unwrap()
            .get(snapshot.custom_meshes[0].geometry_id)
            .is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executes_es_module_entry_with_relative_import() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-module-test-{suffix}"));
        fs::create_dir_all(root.join("node_modules/demo")).unwrap();
        fs::write(root.join("dependency.js"), "export const offset = 2;").unwrap();
        fs::write(
            root.join("node_modules/demo/package.json"),
            r#"{"exports":{".":{"import":"./source.js","default":"./source.js"}}}"#,
        )
        .unwrap();
        fs::write(
            root.join("node_modules/demo/source.js"),
            "export const packageOffset = 3;",
        )
        .unwrap();
        fs::write(
            root.join("main.mjs"),
            r#"
              import { offset } from "./dependency.js";
              import { packageOffset } from "demo";
              globalThis.HyperThreeGame = {
                update() {
                  __hyperthreeBeginFrame();
                  __hyperthreePushCube(offset + packageOffset, 0, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0);
                }
              };
            "#,
        )
        .unwrap();
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state, &root).unwrap();
        runtime.execute_file(root.join("main.mjs")).unwrap();
        runtime.execute_frame(0.0).unwrap();
        let snapshot = render_state.lock().unwrap().snapshot();
        assert_eq!(snapshot.cubes[0].position[0], 5.0);
        fs::remove_dir_all(root).unwrap();
    }
}
