use crate::{
    asset::{decode_meshopt_buffer, AssetStore},
    audio::{decode_audio, AudioEngine},
    bridge::{GeometryKind, MaterialSnapshot, SharedInputState, SharedRenderState},
    draco::decode_mesh as decode_draco_mesh,
    storage::StorageStore,
    webgpu::SharedNativeWebGpuContext,
};
use anyhow::{Context as _, Result};
use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};
use boa_engine::{
    builtins::promise::PromiseState,
    js_string,
    module::{Module, ModuleLoader, Referrer},
    object::{
        builtins::{AlignedVec, JsArray, JsArrayBuffer},
        JsObject,
    },
    Context, JsArgs, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source,
};
use std::{
    cell::RefCell,
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
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
    #[allow(dead_code)]
    pub fn new(
        render_state: SharedRenderState,
        input_state: SharedInputState,
        asset_root: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::new_with_gpu(render_state, input_state, asset_root, None)
    }

    pub fn new_with_gpu(
        render_state: SharedRenderState,
        input_state: SharedInputState,
        asset_root: impl AsRef<Path>,
        gpu: Option<SharedNativeWebGpuContext>,
    ) -> Result<Self> {
        let asset_root = asset_root.as_ref().to_path_buf();
        let asset_store = Arc::new(Mutex::new(AssetStore::new(&asset_root)?));
        let module_loader = Rc::new(ProjectModuleLoader::new(&asset_root)?);
        let mut context = Context::builder()
            .module_loader(module_loader)
            .build()
            .map_err(|error| anyhow::anyhow!("failed to create JavaScript context: {error}"))?;
        let runtime_start = Instant::now();
        let audio_engine = Rc::new(RefCell::new(AudioEngine::default()));
        let storage_store = Rc::new(RefCell::new(StorageStore::new(&asset_root)?));
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
                5,
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

        let particle_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreePushParticle"), 11, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let position = [
                        number_arg(args, 0, context)?,
                        number_arg(args, 1, context)?,
                        number_arg(args, 2, context)?,
                    ];
                    let size = number_arg(args, 3, context)?;
                    let color = [
                        number_arg(args, 4, context)?,
                        number_arg(args, 5, context)?,
                        number_arg(args, 6, context)?,
                        number_arg(args, 7, context)?,
                    ];
                    let emissive = [
                        number_arg(args, 8, context)?,
                        number_arg(args, 9, context)?,
                        number_arg(args, 10, context)?,
                    ];
                    particle_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .push_particle(position, size, color, emissive);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register particle binding: {error}"))?;

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
                4,
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
                        let uv_values = number_array_arg(args, 3, context)?;
                        if uv_values.len() % 2 != 0 {
                            return Err(JsNativeError::range()
                                .with_message("UV attribute length must be divisible by 2")
                                .into());
                        }
                        let uvs = uv_values
                            .chunks_exact(2)
                            .map(|uv| [uv[0] as f32, uv[1] as f32])
                            .collect::<Vec<_>>();
                        let normal_values = number_array_arg(args, 4, context)?;
                        if normal_values.len() % 3 != 0 {
                            return Err(JsNativeError::range()
                                .with_message("normal attribute length must be divisible by 3")
                                .into());
                        }
                        let normals = normal_values
                            .chunks_exact(3)
                            .map(|normal| [normal[0] as f32, normal[1] as f32, normal[2] as f32])
                            .collect::<Vec<_>>();
                        geometry_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .register_geometry(geometry_id, positions, indices, normals, uvs)
                            .map_err(|error| JsNativeError::range().with_message(error))?;
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| anyhow::anyhow!("failed to register geometry binding: {error}"))?;

        let geometry_instance_state = render_state.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreePushGeometry"), 13, unsafe {
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
                    let texture_id = optional_texture_id_arg(args, 12, context)?;
                    geometry_instance_state
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("render state poisoned"))?
                        .push_custom_mesh_with_texture(
                            geometry_id,
                            texture_id,
                            position,
                            scale,
                            rotation_y,
                            color,
                        );
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to register geometry instance binding: {error}")
            })?;

        let material_geometry_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreePushGeometryMaterial"),
                19,
                unsafe {
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
                        let base_color = [
                            number_arg(args, 8, context)?,
                            number_arg(args, 9, context)?,
                            number_arg(args, 10, context)?,
                            number_arg(args, 11, context)?,
                        ];
                        let texture_id = optional_texture_id_arg(args, 12, context)?;
                        let metallic = number_arg(args, 13, context)?;
                        let roughness = number_arg(args, 14, context)?;
                        let emissive = [
                            number_arg(args, 15, context)?,
                            number_arg(args, 16, context)?,
                            number_arg(args, 17, context)?,
                        ];
                        let unlit = number_arg(args, 18, context)? > 0.5;
                        material_geometry_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .push_custom_mesh_with_material(
                                geometry_id,
                                position,
                                scale,
                                rotation_y,
                                MaterialSnapshot {
                                    base_color,
                                    metallic,
                                    roughness,
                                    emissive,
                                    unlit,
                                    base_color_texture: texture_id,
                                },
                            );
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| {
                anyhow::anyhow!("failed to register material geometry binding: {error}")
            })?;

        let matrix_geometry_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreePushGeometryMatrixMaterial"),
                13,
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let geometry_id = geometry_id_arg(args, 0, context)?;
                        let matrix_values = number_array_arg(args, 1, context)?;
                        if matrix_values.len() != 16 {
                            return Err(JsNativeError::range()
                                .with_message("model matrix must contain 16 numbers")
                                .into());
                        }
                        let mut model_matrix = [[0.0; 4]; 4];
                        for column in 0..4 {
                            for row in 0..4 {
                                model_matrix[column][row] = matrix_values[column * 4 + row];
                            }
                        }
                        let base_color = [
                            number_arg(args, 2, context)?,
                            number_arg(args, 3, context)?,
                            number_arg(args, 4, context)?,
                            number_arg(args, 5, context)?,
                        ];
                        let texture_id = optional_texture_id_arg(args, 6, context)?;
                        let metallic = number_arg(args, 7, context)?;
                        let roughness = number_arg(args, 8, context)?;
                        let emissive = [
                            number_arg(args, 9, context)?,
                            number_arg(args, 10, context)?,
                            number_arg(args, 11, context)?,
                        ];
                        let unlit = number_arg(args, 12, context)? > 0.5;
                        matrix_geometry_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .push_custom_mesh_matrix_with_material(
                                geometry_id,
                                model_matrix,
                                MaterialSnapshot {
                                    base_color,
                                    metallic,
                                    roughness,
                                    emissive,
                                    unlit,
                                    base_color_texture: texture_id,
                                },
                            );
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| {
                anyhow::anyhow!("failed to register matrix geometry binding: {error}")
            })?;

        let matrix_primitive_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreePushPrimitiveMatrixMaterial"),
                13,
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let geometry = primitive_kind_arg(args, 0, context)?;
                        let matrix_values = number_array_arg(args, 1, context)?;
                        if matrix_values.len() != 16 {
                            return Err(JsNativeError::range()
                                .with_message("model matrix must contain 16 numbers")
                                .into());
                        }
                        let mut model_matrix = [[0.0; 4]; 4];
                        for column in 0..4 {
                            for row in 0..4 {
                                model_matrix[column][row] = matrix_values[column * 4 + row];
                            }
                        }
                        let material = MaterialSnapshot {
                            base_color: [
                                number_arg(args, 2, context)?,
                                number_arg(args, 3, context)?,
                                number_arg(args, 4, context)?,
                                number_arg(args, 5, context)?,
                            ],
                            base_color_texture: optional_texture_id_arg(args, 6, context)?,
                            metallic: number_arg(args, 7, context)?,
                            roughness: number_arg(args, 8, context)?,
                            emissive: [
                                number_arg(args, 9, context)?,
                                number_arg(args, 10, context)?,
                                number_arg(args, 11, context)?,
                            ],
                            unlit: number_arg(args, 12, context)? > 0.5,
                        };
                        matrix_primitive_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .push_primitive_matrix_with_material(geometry, model_matrix, material);
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| {
                anyhow::anyhow!("failed to register primitive matrix binding: {error}")
            })?;

        let light_state = render_state.clone();
        context
            .register_global_builtin_callable(
                js_string!("__hyperthreeSetDirectionalLight"),
                10,
                unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let direction = [
                            number_arg(args, 0, context)?,
                            number_arg(args, 1, context)?,
                            number_arg(args, 2, context)?,
                        ];
                        let color = [
                            number_arg(args, 3, context)?,
                            number_arg(args, 4, context)?,
                            number_arg(args, 5, context)?,
                        ];
                        let intensity = number_arg(args, 6, context)?;
                        let ambient = [
                            number_arg(args, 7, context)?,
                            number_arg(args, 8, context)?,
                            number_arg(args, 9, context)?,
                        ];
                        light_state
                            .lock()
                            .map_err(|_| {
                                JsNativeError::error().with_message("render state poisoned")
                            })?
                            .set_directional_light(direction, color, intensity, ambient);
                        Ok(JsValue::undefined())
                    })
                },
            )
            .map_err(|error| {
                anyhow::anyhow!("failed to register directional-light binding: {error}")
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

        let asset_store_for_fetch = asset_store.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeReadAsset"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let path = string_arg(args, 0, context)?;
                    let bytes = asset_store_for_fetch
                        .lock()
                        .map_err(|_| JsNativeError::error().with_message("asset store poisoned"))?
                        .read_bytes(&path)
                        .map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
                    let block = AlignedVec::from_iter(0, bytes);
                    let buffer =
                        JsArrayBuffer::from_byte_block(block, context).map_err(|error| {
                            JsNativeError::error().with_message(format!(
                                "failed to create asset ArrayBuffer: {error}"
                            ))
                        })?;
                    Ok(buffer.into())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register asset fetch binding: {error}"))?;

        let storage_for_load = storage_store.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeStorageLoad"), 0, unsafe {
                NativeFunction::from_closure(move |_this, _args, _context| {
                    let payload = storage_for_load.borrow().snapshot_json().map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to load local storage: {error}"))
                    })?;
                    Ok(JsValue::from(JsString::from(payload)))
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register storage load binding: {error}"))?;

        let storage_for_save = storage_store.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeStorageSave"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let payload = string_arg(args, 0, context)?;
                    storage_for_save
                        .borrow_mut()
                        .replace_json(&payload)
                        .map_err(|error| {
                            JsNativeError::error()
                                .with_message(format!("failed to save local storage: {error}"))
                        })?;
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register storage save binding: {error}"))?;

        context
            .register_global_builtin_callable(js_string!("__hyperthreeDecodeAudio"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let source = byte_array_value(args.get_or_undefined(0), context)?;
                    let decoded = decode_audio(&source).map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to decode audio: {error}"))
                    })?;
                    let mut channels = Vec::with_capacity(decoded.channels.len());
                    for channel in decoded.channels {
                        let bytes = bytemuck::cast_slice::<f32, u8>(&channel).to_vec();
                        let buffer = JsArrayBuffer::from_byte_block(
                            AlignedVec::from_iter(0, bytes),
                            context,
                        )
                        .map_err(|error| {
                            JsNativeError::error().with_message(format!(
                                "failed to create audio channel buffer: {error}"
                            ))
                        })?;
                        channels.push(buffer.into());
                    }
                    let result = JsObject::with_object_proto(context.intrinsics());
                    result.set(
                        js_string!("channels"),
                        JsArray::from_iter(channels, context),
                        false,
                        context,
                    )?;
                    result.set(
                        js_string!("sampleRate"),
                        JsValue::from(decoded.sample_rate as f64),
                        false,
                        context,
                    )?;
                    result.set(
                        js_string!("length"),
                        JsValue::from(decoded.length as f64),
                        false,
                        context,
                    )?;
                    result.set(
                        js_string!("duration"),
                        JsValue::from(decoded.length as f64 / decoded.sample_rate as f64),
                        false,
                        context,
                    )?;
                    Ok(result.into())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register audio decode binding: {error}"))?;

        let audio_engine_for_play = audio_engine.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeAudioPlay"), 6, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let source = byte_array_value(args.get_or_undefined(0), context)?;
                    let looped = args.get_or_undefined(1).to_boolean();
                    let volume = optional_number_arg(args, 2, context, 1.0)? as f32;
                    let when = optional_number_arg(args, 3, context, 0.0)?;
                    let offset = optional_number_arg(args, 4, context, 0.0)?;
                    let duration = optional_number_arg(args, 5, context, 0.0)?;
                    let id = audio_engine_for_play
                        .borrow_mut()
                        .play(source, looped, volume, when, offset, duration)
                        .map_err(|error| {
                            JsNativeError::error()
                                .with_message(format!("native audio playback failed: {error}"))
                        })?;
                    Ok(JsValue::from(id as f64))
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to register audio playback binding: {error}")
            })?;

        for (name, operation) in [
            ("__hyperthreeAudioStop", 0u8),
            ("__hyperthreeAudioPause", 1u8),
            ("__hyperthreeAudioResume", 2u8),
        ] {
            let audio_engine_for_control = audio_engine.clone();
            context
                .register_global_builtin_callable(js_string!(name), 1, unsafe {
                    NativeFunction::from_closure(move |_this, args, context| {
                        let id = geometry_id_arg(args, 0, context)?;
                        let mut engine = audio_engine_for_control.borrow_mut();
                        match operation {
                            0 => engine.stop(id),
                            1 => engine.pause(id),
                            _ => engine.resume(id),
                        }
                        Ok(JsValue::undefined())
                    })
                })
                .map_err(|error| {
                    anyhow::anyhow!("failed to register audio control binding {name}: {error}")
                })?;
        }

        let audio_engine_for_volume = audio_engine.clone();
        context
            .register_global_builtin_callable(js_string!("__hyperthreeAudioSetVolume"), 2, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let id = geometry_id_arg(args, 0, context)?;
                    let volume = number_arg(args, 1, context)? as f32;
                    audio_engine_for_volume.borrow_mut().set_volume(id, volume);
                    Ok(JsValue::undefined())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register audio volume binding: {error}"))?;

        context
            .register_global_builtin_callable(js_string!("__hyperthreeDecodeMeshopt"), 5, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let source = byte_array_value(args.get_or_undefined(0), context)?;
                    let count = nonnegative_usize_arg(args, 1, context)?;
                    let stride = nonnegative_usize_arg(args, 2, context)?;
                    let mode = string_arg(args, 3, context)?;
                    let filter_value = args.get_or_undefined(4);
                    let filter = if filter_value.is_undefined() || filter_value.is_null() {
                        "NONE".to_string()
                    } else {
                        filter_value
                            .to_string(context)
                            .map_err(|_| {
                                JsNativeError::typ().with_message("meshopt filter is invalid")
                            })?
                            .to_std_string_escaped()
                    };
                    let decoded = decode_meshopt_buffer(&source, count, stride, &mode, &filter)
                        .map_err(|error| {
                            JsNativeError::error().with_message(format!(
                                "{error} (source={}, count={count}, stride={stride}, mode={mode}, filter={filter})",
                                source.len()
                            ))
                        })?;
                    let block = AlignedVec::from_iter(0, decoded);
                    JsArrayBuffer::from_byte_block(block, context)
                        .map(Into::into)
                        .map_err(|error| {
                            JsNativeError::error()
                                .with_message(format!("failed to create meshopt buffer: {error}"))
                                .into()
                        })
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register meshopt binding: {error}"))?;

        context
            .register_global_builtin_callable(js_string!("__hyperthreeDecodeDraco"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let source = byte_array_value(args.get_or_undefined(0), context)?;
                    let geometry = decode_draco_mesh(&source).map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to decode Draco geometry: {error}"))
                    })?;

                    let index_bytes = bytemuck::cast_slice::<u32, u8>(&geometry.indices).to_vec();
                    let index = JsArrayBuffer::from_byte_block(
                        AlignedVec::from_iter(0, index_bytes),
                        context,
                    )
                    .map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to create Draco index buffer: {error}"))
                    })?;
                    let mut attributes = Vec::with_capacity(geometry.attributes.len());
                    for attribute in geometry.attributes {
                        let data_bytes = bytemuck::cast_slice::<f32, u8>(&attribute.data).to_vec();
                        let data = JsArrayBuffer::from_byte_block(
                            AlignedVec::from_iter(0, data_bytes),
                            context,
                        )
                        .map_err(|error| {
                            JsNativeError::error().with_message(format!(
                                "failed to create Draco attribute buffer: {error}"
                            ))
                        })?;
                        let value = JsObject::with_object_proto(context.intrinsics());
                        value.set(
                            js_string!("name"),
                            js_string!(attribute.name),
                            false,
                            context,
                        )?;
                        value.set(
                            js_string!("itemSize"),
                            JsValue::from(attribute.item_size as f64),
                            false,
                            context,
                        )?;
                        value.set(js_string!("data"), data, false, context)?;
                        attributes.push(value.into());
                    }
                    let result = JsObject::with_object_proto(context.intrinsics());
                    result.set(js_string!("index"), index, false, context)?;
                    result.set(
                        js_string!("attributes"),
                        JsArray::from_iter(attributes, context),
                        false,
                        context,
                    )?;
                    Ok(result.into())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register Draco binding: {error}"))?;

        context
            .register_global_builtin_callable(js_string!("__hyperthreeTranscodeKtx2"), 2, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let source = byte_array_value(args.get_or_undefined(0), context)?;

                    // A non-zero vkFormat identifies a KTX2 file whose payload is
                    // already GPU-ready. Shape the raw levels directly so the
                    // standard KTX2Loader worker is not required in the native host.
                    if source.len() < 16 {
                        return Ok(JsValue::null());
                    }
                    if u32::from_le_bytes([source[12], source[13], source[14], source[15]]) != 0 {
                        return raw_ktx2_result(&source, context);
                    }

                    let transcoder = Transcoder::new(&source).map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to open Basis/KTX2 texture: {error:?}"))
                    })?;
                    let config = args.get_or_undefined(1);
                    let has_alpha = transcoder.has_alpha();
                    let (target, format, type_name) = choose_basis_target(
                        &transcoder,
                        has_alpha,
                        js_bool_property(config, "astcSupported", context)?,
                        js_bool_property(config, "bptcSupported", context)?,
                        js_bool_property(config, "dxtSupported", context)?,
                        js_bool_property(config, "etc2Supported", context)?,
                    )
                    .map_err(|error| JsNativeError::error().with_message(error))?;
                    let (width, height) = transcoder.base_dimensions();
                    let level_count = transcoder.level_count();
                    let layer_count = transcoder.layer_count().max(1);
                    let face_count = transcoder.face_count().max(1);

                    let mut faces = Vec::with_capacity(face_count as usize);
                    for face in 0..face_count {
                        let mut mipmaps = Vec::with_capacity(level_count as usize);
                        for level in 0..level_count {
                            let info = transcoder.image_level_info(level).map_err(|error| {
                                JsNativeError::error().with_message(format!(
                                    "failed to inspect Basis/KTX2 mip level {level}: {error:?}"
                                ))
                            })?;
                            let mut mip_bytes = Vec::new();
                            for layer in 0..layer_count {
                                let bytes = transcoder
                                    .transcode_image(
                                        level,
                                        layer,
                                        face,
                                        target,
                                        DecodeFlags::NONE,
                                    )
                                    .map_err(|error| {
                                        JsNativeError::error().with_message(format!(
                                            "failed to transcode Basis/KTX2 level {level}, layer {layer}, face {face} to {target:?}: {error:?}"
                                        ))
                                    })?;
                                mip_bytes.extend_from_slice(&bytes);
                            }
                            let block = AlignedVec::from_iter(0, mip_bytes);
                            let buffer = JsArrayBuffer::from_byte_block(block, context).map_err(
                                |error| {
                                    JsNativeError::error().with_message(format!(
                                        "failed to create transcoded KTX2 buffer: {error}"
                                    ))
                                },
                            )?;
                            let mipmap = JsObject::with_object_proto(context.intrinsics());
                            mipmap.set(js_string!("data"), buffer, false, context)?;
                            mipmap.set(
                                js_string!("width"),
                                JsValue::from(info.width as f64),
                                false,
                                context,
                            )?;
                            mipmap.set(
                                js_string!("height"),
                                JsValue::from(info.height as f64),
                                false,
                                context,
                            )?;
                            mipmaps.push(mipmap.into());
                        }
                        let face_value = JsObject::with_object_proto(context.intrinsics());
                        face_value.set(
                            js_string!("mipmaps"),
                            JsArray::from_iter(mipmaps, context),
                            false,
                            context,
                        )?;
                        faces.push(face_value.into());
                    }

                    let data = JsObject::with_object_proto(context.intrinsics());
                    data.set(
                        js_string!("faces"),
                        JsArray::from_iter(faces, context),
                        false,
                        context,
                    )?;
                    data.set(
                        js_string!("width"),
                        JsValue::from(width as f64),
                        false,
                        context,
                    )?;
                    data.set(
                        js_string!("height"),
                        JsValue::from(height as f64),
                        false,
                        context,
                    )?;
                    data.set(js_string!("format"), js_string!(format), false, context)?;
                    data.set(js_string!("type"), js_string!(type_name), false, context)?;
                    data.set(js_string!("dfdFlags"), JsValue::from(0), false, context)?;
                    let result = JsObject::with_object_proto(context.intrinsics());
                    result.set(js_string!("type"), js_string!("transcode"), false, context)?;
                    result.set(js_string!("data"), data, false, context)?;
                    Ok(result.into())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register KTX2 transcoder binding: {error}"))?;

        context
            .register_global_builtin_callable(js_string!("__hyperthreeDecodeImage"), 1, unsafe {
                NativeFunction::from_closure(move |_this, args, context| {
                    let source = args.get_or_undefined(0).to_object(context).map_err(|_| {
                        JsNativeError::typ().with_message("image source must be a Blob")
                    })?;
                    let bytes = source
                        .get(js_string!("__hyperthreeBytes"), context)
                        .and_then(|value| byte_array_value(&value, context))?;
                    let decoded = image::load_from_memory(&bytes).map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to decode image bytes: {error}"))
                    })?;
                    let rgba = decoded.to_rgba8();
                    let width = rgba.width();
                    let height = rgba.height();
                    let block = AlignedVec::from_iter(0, rgba.into_raw());
                    let data = JsArrayBuffer::from_byte_block(block, context).map_err(|error| {
                        JsNativeError::error()
                            .with_message(format!("failed to create decoded image buffer: {error}"))
                    })?;
                    let image = JsObject::with_object_proto(context.intrinsics());
                    image.set(
                        js_string!("width"),
                        JsValue::from(width as f64),
                        false,
                        context,
                    )?;
                    image.set(
                        js_string!("height"),
                        JsValue::from(height as f64),
                        false,
                        context,
                    )?;
                    image.set(js_string!("data"), data, false, context)?;
                    Ok(image.into())
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to register image decode binding: {error}"))?;

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
                            geometry.normals.clone(),
                            geometry.uvs.clone(),
                        )
                        .map_err(|error| JsNativeError::range().with_message(error))?;
                    let texture_id = if let Some(texture) = &geometry.texture {
                        state
                            .register_texture(
                                texture.texture_id,
                                texture.width,
                                texture.height,
                                texture.rgba8.clone(),
                            )
                            .map_err(|error| JsNativeError::range().with_message(error))?;
                        Some(texture.texture_id)
                    } else {
                        None
                    };
                    state.push_custom_mesh_with_material(
                        geometry.geometry_id,
                        position,
                        scale,
                        rotation_y,
                        MaterialSnapshot {
                            base_color: [
                                color[0] * geometry.material.base_color[0],
                                color[1] * geometry.material.base_color[1],
                                color[2] * geometry.material.base_color[2],
                                color[3] * geometry.material.base_color[3],
                            ],
                            metallic: geometry.material.metallic,
                            roughness: geometry.material.roughness,
                            emissive: geometry.material.emissive,
                            unlit: geometry.material.unlit,
                            base_color_texture: texture_id,
                        },
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
                globalThis.Event = globalThis.Event || class Event {
                  constructor(type, init = {}) {
                    this.type = String(type);
                    this.bubbles = Boolean(init.bubbles);
                    this.cancelable = Boolean(init.cancelable);
                    this.defaultPrevented = false;
                  }
                  preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
                };
                globalThis.__hyperthreeEventListeners = new Map();
                globalThis.addEventListener = (type, listener) => {
                  if (typeof listener !== 'function') return;
                  const listeners = globalThis.__hyperthreeEventListeners.get(type) || new Set();
                  listeners.add(listener);
                  globalThis.__hyperthreeEventListeners.set(type, listeners);
                };
                globalThis.removeEventListener = (type, listener) => {
                  globalThis.__hyperthreeEventListeners.get(type)?.delete(listener);
                };
                globalThis.dispatchEvent = (event) => {
                  for (const listener of globalThis.__hyperthreeEventListeners.get(event.type) || []) listener.call(globalThis, event);
                  return !event.defaultPrevented;
                };
                globalThis.console = globalThis.console || {
                  log() {},
                  info() {},
                  warn() {},
                  error() {},
                  debug() {},
                };
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
                globalThis.Headers = globalThis.Headers || class Headers {
                  constructor(init = {}) {
                    this.__values = new Map();
                    for (const [key, value] of Object.entries(init)) this.__values.set(String(key).toLowerCase(), String(value));
                  }
                  get(key) { return this.__values.get(String(key).toLowerCase()) ?? null; }
                  has(key) { return this.__values.has(String(key).toLowerCase()); }
                };
                globalThis.Blob = globalThis.Blob || class Blob {
                  constructor(parts = [], options = {}) {
                    const chunks = parts.map((part) => {
                      if (part instanceof ArrayBuffer) return new Uint8Array(part);
                      if (ArrayBuffer.isView(part)) return new Uint8Array(part.buffer, part.byteOffset || 0, part.byteLength);
                      if (part && part.__hyperthreeBytes) return part.__hyperthreeBytes;
                      const text = String(part);
                      const bytes = new Uint8Array(text.length);
                      for (let index = 0; index < text.length; index += 1) bytes[index] = text.charCodeAt(index) & 0xff;
                      return bytes;
                    });
                    const size = chunks.reduce((total, chunk) => total + chunk.byteLength, 0);
                    this.__hyperthreeBytes = new Uint8Array(size);
                    let offset = 0;
                    for (const chunk of chunks) { this.__hyperthreeBytes.set(chunk, offset); offset += chunk.byteLength; }
                    this.size = size;
                    this.type = String(options.type || '').toLowerCase();
                  }
                  async arrayBuffer() { return this.__hyperthreeBytes.slice().buffer; }
                  async text() { return new TextDecoder().decode(this.__hyperthreeBytes); }
                  slice(start = 0, end = this.size, contentType = '') {
                    const from = Math.max(0, Number(start) || 0);
                    const to = Math.max(from, Number(end) || this.size);
                    return new Blob([this.__hyperthreeBytes.slice(from, to)], { type: contentType });
                  }
                };
                globalThis.File = globalThis.File || class File extends Blob {
                  constructor(parts, name, options = {}) {
                    super(parts, options);
                    this.name = String(name);
                    this.lastModified = Number(options.lastModified || 0);
                  }
                };
                const makeStorage = (persistent) => {
                  const values = new Map();
                  if (persistent) {
                    const saved = JSON.parse(__hyperthreeStorageLoad() || '{}');
                    for (const [key, value] of Object.entries(saved)) values.set(String(key), String(value));
                  }
                  const persist = () => {
                    if (!persistent) return;
                    const saved = {};
                    for (const [key, value] of values) saved[key] = value;
                    __hyperthreeStorageSave(JSON.stringify(saved));
                  };
                  return {
                    get length() { return values.size; },
                    key(index) { return Array.from(values.keys())[Number(index)] ?? null; },
                    getItem(key) { return values.get(String(key)) ?? null; },
                    setItem(key, value) { values.set(String(key), String(value)); persist(); },
                    removeItem(key) { values.delete(String(key)); persist(); },
                    clear() { values.clear(); persist(); },
                  };
                };
                globalThis.localStorage = globalThis.localStorage || makeStorage(true);
                globalThis.sessionStorage = globalThis.sessionStorage || makeStorage(false);
                globalThis.__hyperthreeBlobUrls = globalThis.__hyperthreeBlobUrls || new Map();
                globalThis.__hyperthreeBlobUrlId = globalThis.__hyperthreeBlobUrlId || 0;
                globalThis.URL = globalThis.URL || class URL {
                  constructor(input, base = undefined) {
                    const value = String(input);
                    this.href = base === undefined ? value : new URL(base).href.replace(/[^/]*$/, '') + value;
                    this.protocol = this.href.includes(':') ? this.href.slice(0, this.href.indexOf(':') + 1) : '';
                    this.pathname = this.href.slice(this.protocol.length).split(/[?#]/)[0];
                    this.search = this.href.includes('?') ? `?${this.href.split('?')[1].split('#')[0]}` : '';
                    this.hash = this.href.includes('#') ? `#${this.href.split('#')[1]}` : '';
                  }
                  toString() { return this.href; }
                  static createObjectURL(value) {
                    const id = `blob:hyperthree/${++globalThis.__hyperthreeBlobUrlId}`;
                    const bytes = value?.__hyperthreeBytes || new Uint8Array(0);
                    globalThis.__hyperthreeBlobUrls.set(id, new Uint8Array(bytes));
                    return id;
                  }
                  static revokeObjectURL(id) { globalThis.__hyperthreeBlobUrls.delete(String(id)); }
                };
                const makeAudioParam = (owner, initialValue = 0, onChange = () => {}) => {
                  const param = {
                    value: Number(initialValue),
                    defaultValue: Number(initialValue),
                    setValueAtTime(value) { param.value = Number(value); onChange(param.value); return param; },
                    setTargetAtTime(value) { param.value = Number(value); onChange(param.value); return param; },
                    linearRampToValueAtTime(value) { param.value = Number(value); onChange(param.value); return param; },
                    exponentialRampToValueAtTime(value) { param.value = Number(value); onChange(param.value); return param; },
                    cancelScheduledValues() { return param; },
                  };
                  owner?.__hyperthreeAudioParams?.push(param);
                  return param;
                };
                globalThis.AudioBuffer = globalThis.AudioBuffer || class AudioBuffer {
                  constructor(options = {}) {
                    this.sampleRate = Number(options.sampleRate || 44100);
                    this.length = Number(options.length || 0);
                    this.duration = this.length / this.sampleRate;
                    this.numberOfChannels = Number(options.numberOfChannels || options.channels?.length || 1);
                    this._channels = options.channels || Array.from({ length: this.numberOfChannels }, () => new Float32Array(this.length));
                    this.__hyperthreeEncoded = options.encoded || null;
                  }
                  getChannelData(channel) { return this._channels[channel] || new Float32Array(0); }
                  copyFromChannel(destination, channel, startInChannel = 0) {
                    destination.set(this.getChannelData(channel).subarray(startInChannel, startInChannel + destination.length));
                  }
                  copyToChannel(source, channel, startInChannel = 0) {
                    this.getChannelData(channel).set(source, startInChannel);
                  }
                };
                globalThis.AudioNode = globalThis.AudioNode || class AudioNode {
                  constructor(context) { this.context = context; this._destination = null; }
                  connect(destination) { this._destination = destination; return destination; }
                  disconnect() { this._destination = null; }
                };
                globalThis.GainNode = globalThis.GainNode || class GainNode extends AudioNode {
                  constructor(context) {
                    super(context);
                    this.__hyperthreeSourceIds = [];
                    this.__hyperthreeAudioParams = [];
                    this.gain = makeAudioParam(this, 1, (value) => {
                      for (const id of this.__hyperthreeSourceIds) __hyperthreeAudioSetVolume(id, value);
                    });
                  }
                };
                globalThis.AudioBufferSourceNode = globalThis.AudioBufferSourceNode || class AudioBufferSourceNode extends AudioNode {
                  constructor(context) {
                    super(context);
                    this.buffer = null;
                    this.loop = false;
                    this.loopStart = 0;
                    this.loopEnd = 0;
                    this.onended = null;
                    this.__hyperthreeStarted = false;
                    this.__hyperthreeAudioParams = [];
                    this.playbackRate = makeAudioParam(this, 1);
                    this.detune = makeAudioParam(this, 0);
                  }
                  start(when = 0, offset = 0, duration = 0) {
                    if (this.__hyperthreeStarted || !this.buffer?.__hyperthreeEncoded) return;
                    this.__hyperthreeStarted = true;
                    const destination = this._destination;
                    const volume = destination?.gain?.value ?? 1;
                    const id = __hyperthreeAudioPlay(this.buffer.__hyperthreeEncoded, this.loop, volume, when, offset, duration);
                    this.__hyperthreeAudioId = id;
                    if (destination?.__hyperthreeSourceIds) destination.__hyperthreeSourceIds.push(id);
                  }
                  stop() {
                    if (this.__hyperthreeAudioId !== undefined) __hyperthreeAudioStop(this.__hyperthreeAudioId);
                    if (typeof this.onended === 'function') this.onended();
                  }
                };
                globalThis.PannerNode = globalThis.PannerNode || class PannerNode extends AudioNode {
                  constructor(context) {
                    super(context);
                    this.panningModel = 'HRTF';
                    this.distanceModel = 'inverse';
                    this.refDistance = 1;
                    this.maxDistance = 10000;
                    this.rolloffFactor = 1;
                    this.coneInnerAngle = 360;
                    this.coneOuterAngle = 0;
                    this.coneOuterGain = 0;
                    this.positionX = makeAudioParam(this, 0);
                    this.positionY = makeAudioParam(this, 0);
                    this.positionZ = makeAudioParam(this, 0);
                    this.orientationX = makeAudioParam(this, 1);
                    this.orientationY = makeAudioParam(this, 0);
                    this.orientationZ = makeAudioParam(this, 0);
                  }
                  setPosition(x, y, z) { this.positionX.value = x; this.positionY.value = y; this.positionZ.value = z; }
                  setOrientation(x, y, z) { this.orientationX.value = x; this.orientationY.value = y; this.orientationZ.value = z; }
                };
                globalThis.AudioContext = globalThis.AudioContext || class AudioContext {
                  constructor() {
                    this.sampleRate = 44100;
                    this.state = 'running';
                    this.destination = { context: this, __hyperthreeSourceIds: [] };
                    this.listener = {
                      positionX: makeAudioParam(null, 0), positionY: makeAudioParam(null, 0), positionZ: makeAudioParam(null, 0),
                      forwardX: makeAudioParam(null, 0), forwardY: makeAudioParam(null, 0), forwardZ: makeAudioParam(null, -1),
                      upX: makeAudioParam(null, 0), upY: makeAudioParam(null, 1), upZ: makeAudioParam(null, 0),
                      setPosition() {}, setOrientation() {},
                    };
                    this.__hyperthreeAudioStart = performance.now() / 1000;
                  }
                  get currentTime() { return Math.max(0, performance.now() / 1000 - this.__hyperthreeAudioStart); }
                  createBuffer(numberOfChannels, length, sampleRate) { return new AudioBuffer({ numberOfChannels, length, sampleRate }); }
                  decodeAudioData(data, successCallback, errorCallback) {
                    const promise = Promise.resolve().then(() => {
                      const decoded = __hyperthreeDecodeAudio(new Uint8Array(data));
                      const buffer = new AudioBuffer({
                        numberOfChannels: decoded.channels.length,
                        length: decoded.length,
                        sampleRate: decoded.sampleRate,
                        channels: decoded.channels.map((channel) => new Float32Array(channel)),
                        encoded: data.slice(0),
                      });
                      if (typeof successCallback === 'function') successCallback(buffer);
                      return buffer;
                    });
                    if (typeof errorCallback === 'function') promise.catch(errorCallback);
                    return promise;
                  }
                  createBufferSource() { return new AudioBufferSourceNode(this); }
                  createGain() { return new GainNode(this); }
                  createPanner() { return new PannerNode(this); }
                  resume() { this.state = 'running'; return Promise.resolve(); }
                  suspend() { this.state = 'suspended'; return Promise.resolve(); }
                  close() { this.state = 'closed'; return Promise.resolve(); }
                };
                globalThis.webkitAudioContext = globalThis.webkitAudioContext || globalThis.AudioContext;
                globalThis.Response = globalThis.Response || class Response {
                  constructor(body = null, init = {}) {
                    this._body = body instanceof ArrayBuffer ? body : new Uint8Array(body || []).buffer;
                    this.url = init.url || '';
                    this.status = init.status ?? 200;
                    this.statusText = init.statusText || 'OK';
                    this.ok = this.status >= 200 && this.status < 300;
                    this.headers = init.headers || new Headers();
                    this.body = undefined;
                  }
                  async arrayBuffer() { return this._body.slice(0); }
                  async text() { return new TextDecoder().decode(this._body); }
                  async json() { return JSON.parse(await this.text()); }
                  async blob() { return new Blob([this._body], { type: this.headers.get('Content-Type') || '' }); }
                };
                globalThis.Request = globalThis.Request || class Request {
                  constructor(input, init = {}) {
                    this.url = typeof input === 'string' ? input : input.url;
                    this.method = init.method || (typeof input === 'object' && input.method) || 'GET';
                    this.headers = init.headers || (typeof input === 'object' && input.headers) || new Headers();
                    this.signal = init.signal || (typeof input === 'object' && input.signal) || null;
                  }
                };
                globalThis.AbortController = globalThis.AbortController || class AbortController {
                  constructor() { this.signal = { aborted: false }; }
                  abort() { this.signal.aborted = true; }
                };
                globalThis.AbortSignal = globalThis.AbortSignal || {
                  any(signals) { return signals[0] || { aborted: false }; }
                };
                globalThis.TextDecoder = globalThis.TextDecoder || class TextDecoder {
                  decode(input) {
                    const bytes = input instanceof ArrayBuffer ? new Uint8Array(input) : new Uint8Array(input.buffer, input.byteOffset || 0, input.byteLength);
                    let value = '';
                    for (let index = 0; index < bytes.length; index += 1) {
                      const first = bytes[index];
                      if (first < 0x80) { value += String.fromCharCode(first); continue; }
                      if ((first & 0xe0) === 0xc0 && index + 1 < bytes.length) {
                        value += String.fromCodePoint(((first & 0x1f) << 6) | (bytes[++index] & 0x3f));
                        continue;
                      }
                      if ((first & 0xf0) === 0xe0 && index + 2 < bytes.length) {
                        value += String.fromCodePoint(((first & 0x0f) << 12) | ((bytes[++index] & 0x3f) << 6) | (bytes[++index] & 0x3f));
                        continue;
                      }
                      if ((first & 0xf8) === 0xf0 && index + 3 < bytes.length) {
                        value += String.fromCodePoint(((first & 0x07) << 18) | ((bytes[++index] & 0x3f) << 12) | ((bytes[++index] & 0x3f) << 6) | (bytes[++index] & 0x3f));
                      }
                    }
                    return value;
                  }
                };
                globalThis.ImageBitmap = globalThis.ImageBitmap || function ImageBitmap() {};
                globalThis.createImageBitmap = globalThis.createImageBitmap || (async (source) => {
                  const decoded = __hyperthreeDecodeImage(source);
                  const bitmap = {
                    width: decoded.width,
                    height: decoded.height,
                    data: new Uint8Array(decoded.data),
                    close() { this.data = new Uint8Array(0); },
                  };
                  Object.setPrototypeOf(bitmap, ImageBitmap.prototype);
                  return bitmap;
                });
                globalThis.__hyperthreeMeshoptDecoder = globalThis.__hyperthreeMeshoptDecoder || {
                  supported: true,
                  ready: Promise.resolve(),
                  useWorkers() {},
                  decodeGltfBufferAsync(count, stride, source, mode, filter = 'NONE') {
                    return Promise.resolve(new Uint8Array(__hyperthreeDecodeMeshopt(source, count, stride, mode, filter)));
                  },
                  decodeGltfBuffer(target, count, stride, source, mode, filter = 'NONE') {
                    const decoded = new Uint8Array(__hyperthreeDecodeMeshopt(source, count, stride, mode, filter));
                    target.set(decoded);
                  },
                };
                globalThis.__hyperthreeKtx2NativeCalls = globalThis.__hyperthreeKtx2NativeCalls || 0;
                globalThis.__hyperthreeKtx2Transcode = globalThis.__hyperthreeKtx2Transcode || ((buffer, config) => { globalThis.__hyperthreeKtx2NativeCalls += 1; const nativeConfig = Object.assign({}, config || {}); nativeConfig.bptcSupported = nativeConfig.bptcSupported || nativeConfig.dxtSupported; return Promise.resolve(__hyperthreeTranscodeKtx2(buffer, nativeConfig)); });
                const decodeDataUrl = (url) => {
                  const comma = url.indexOf(',');
                  if (comma < 0) throw new TypeError(`invalid data URL: ${url}`);
                  const metadata = url.slice(0, comma).toLowerCase();
                  const payload = url.slice(comma + 1);
                  if (metadata.endsWith(';base64')) {
                    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
                    const output = [];
                    let accumulator = 0;
                    let bits = 0;
                    for (const character of payload) {
                      if (character === '=') break;
                      const value = alphabet.indexOf(character);
                      if (value < 0) continue;
                      accumulator = (accumulator << 6) | value;
                      bits += 6;
                      if (bits >= 8) {
                        bits -= 8;
                        output.push((accumulator >> bits) & 0xff);
                      }
                    }
                    return new Uint8Array(output).buffer;
                  }
                  const text = decodeURIComponent(payload);
                  const bytes = new Uint8Array(text.length);
                  for (let index = 0; index < text.length; index += 1) bytes[index] = text.charCodeAt(index) & 0xff;
                  return bytes.buffer;
                };
                globalThis.fetch = globalThis.fetch || (async (input) => {
                  const request = input instanceof Request ? input : new Request(input);
                  const buffer = request.url.startsWith('data:')
                    ? decodeDataUrl(request.url)
                    : request.url.startsWith('blob:')
                      ? (globalThis.__hyperthreeBlobUrls.get(request.url)?.slice().buffer || new ArrayBuffer(0))
                      : __hyperthreeReadAsset(request.url);
                  const headers = new Headers({ 'Content-Length': buffer.byteLength });
                  return new Response(buffer, { url: request.url, headers });
                });
                "#,
            ))
            .map(|_| ())
            .map_err(|error| {
                anyhow::anyhow!("failed to install runtime compatibility globals: {error}")
            })?;
        crate::webgpu::register_bindings(&mut context, gpu)?;

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
        let source = normalize_three_compatibility_source(source);
        let evaluated = catch_unwind(AssertUnwindSafe(|| {
            self.context.eval(Source::from_bytes(source.as_ref()))
        }))
        .map_err(|panic| {
            anyhow::anyhow!("JavaScript evaluation panicked: {}", panic_message(panic))
        })?;
        evaluated
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("JavaScript evaluation failed: {error}"))?;
        catch_unwind(AssertUnwindSafe(|| self.context.run_jobs()))
            .map_err(|panic| anyhow::anyhow!("JavaScript jobs panicked: {}", panic_message(panic)))?
            .map_err(|error| anyhow::anyhow!("JavaScript jobs failed: {error}"))?;
        Ok(())
    }

    fn execute_module(&mut self, path: &Path, source: &str) -> Result<()> {
        let source = normalize_three_compatibility_source(source);
        let module = catch_unwind(AssertUnwindSafe(|| {
            Module::parse(
                Source::from_bytes(source.as_ref()).with_path(path),
                None,
                &mut self.context,
            )
        }))
        .map_err(|panic| {
            anyhow::anyhow!("JavaScript module parse panicked: {}", panic_message(panic))
        })?
        .map_err(|error| anyhow::anyhow!("JavaScript module parse failed: {error}"))?;
        let promise = catch_unwind(AssertUnwindSafe(|| {
            module.load_link_evaluate(&mut self.context)
        }))
        .map_err(|panic| {
            anyhow::anyhow!(
                "JavaScript module evaluation panicked: {}",
                panic_message(panic)
            )
        })?;
        catch_unwind(AssertUnwindSafe(|| self.context.run_jobs()))
            .map_err(|panic| {
                anyhow::anyhow!("JavaScript module jobs panicked: {}", panic_message(panic))
            })?
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

    pub fn set_window_size(&mut self, width: u32, height: u32) -> Result<()> {
        let width = width.max(1);
        let height = height.max(1);
        self.execute_source(&format!(
            "globalThis.window.innerWidth={width}; globalThis.window.innerHeight={height}; globalThis.__hyperthreeNativeCanvas.clientWidth={width}; globalThis.__hyperthreeNativeCanvas.clientHeight={height}; globalThis.dispatchEvent(new Event('resize'));"
        ))
    }

    fn execute_lifecycle_callback(&mut self, callback: &str) -> Result<()> {
        let source = format!(
            "if (typeof globalThis.HyperThreeGame !== 'undefined' && typeof globalThis.HyperThreeGame.{callback} === 'function') globalThis.HyperThreeGame.{callback}();"
        );
        self.execute_source(&source)
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn normalize_three_compatibility_source(source: &str) -> std::borrow::Cow<'_, str> {
    let minified = "}get(e){let t=this.weakMap;";
    let readable = "get( keys ) {\n\n\t\tlet map = this.weakMap;";
    let mut normalized = if source.contains(minified) {
        source.replace(
            minified,
            "}get(e){if(e.length===0)return;let t=this.weakMap;",
        )
    } else if source.contains(readable) {
        source.replace(
            readable,
            "get( keys ) {\n\n\t\tif ( keys.length === 0 ) return undefined;\n\n\t\tlet map = this.weakMap;",
        )
    } else {
        source.to_string()
    };

    let is_gltf_loader_bundle = source.contains("GLTFLoader")
        || source.contains("THREE.GLTFLoader")
        || source.contains("dracoLoader.preload");
    let is_ktx2_loader_bundle = source.contains("KTX2Loader");
    let is_draco_loader_bundle = source.contains("DRACOLoader")
        || (source.contains("decodeDracoFile") && source.contains("draco_decoder"));
    let mut boa_changed = false;
    if source.contains("decodeAudioData") {
        boa_changed |= normalize_boa_audio_context_binding(&mut normalized);
    }
    if is_gltf_loader_bundle {
        if normalized.contains("let of=") {
            normalized = normalized.replace("let of=", "var of=");
            boa_changed = true;
        }
        for (from, to) in [
            (
                "this.meshoptDecoder = null;",
                "this.meshoptDecoder = globalThis.__hyperthreeMeshoptDecoder || null;",
            ),
            (
                "this.meshoptDecoder=null;",
                "this.meshoptDecoder=globalThis.__hyperthreeMeshoptDecoder||null;",
            ),
            (
                "this.meshoptDecoder=null,",
                "this.meshoptDecoder=globalThis.__hyperthreeMeshoptDecoder||null,",
            ),
        ] {
            if normalized.contains(from) {
                normalized = normalized.replace(from, to);
                boa_changed = true;
            }
        }
        for (from, to) in [
            (
                "this.dracoLoader.preload();",
                "if ( ! globalThis.__hyperthreeDecodeDraco ) this.dracoLoader.preload();",
            ),
            (
                "this.dracoLoader.preload()",
                "globalThis.__hyperthreeDecodeDraco ? this.dracoLoader : this.dracoLoader.preload()",
            ),
        ] {
            if normalized.contains(from) {
                normalized = normalized.replace(from, to);
                boa_changed = true;
            }
        }
    }
    if is_ktx2_loader_bundle {
        for (from, to) in [
            (
                "const container = read( new Uint8Array( buffer ) );",
                "const container = read( new Uint8Array( buffer ) );\n\n\t\tif ( globalThis.__hyperthreeKtx2Transcode ) {\n\n\t\t\tconst nativeResult = await globalThis.__hyperthreeKtx2Transcode( new Uint8Array( buffer ), this.workerConfig || {} );\n\n\t\t\tif ( nativeResult ) {\n\n\t\t\t\tnativeResult.data.format = KTX2Loader.EngineFormat[ nativeResult.data.format ];\n\t\t\t\tnativeResult.data.type = KTX2Loader.EngineType[ nativeResult.data.type ];\n\t\t\t\treturn this._createTextureFrom( nativeResult, container );\n\n\t\t\t}\n\n\t\t}"
            ),
            (
                "const container=read(new Uint8Array(buffer));",
                "const container=read(new Uint8Array(buffer));if(globalThis.__hyperthreeKtx2Transcode){const nativeResult=await globalThis.__hyperthreeKtx2Transcode(new Uint8Array(buffer),this.workerConfig||{});if(nativeResult){nativeResult.data.format=KTX2Loader.EngineFormat[nativeResult.data.format];nativeResult.data.type=KTX2Loader.EngineType[nativeResult.data.type];return this._createTextureFrom(nativeResult,container);}}"
            ),
        ] {
            if normalized.contains(from) {
                normalized = normalized.replace(from, to);
                boa_changed = true;
            }
        }
        let minified_ktx_from = "async _createTexture(e,t={}){const s=oS(new Uint8Array(e)),i=";
        let minified_ktx_to = r#"async _createTexture(e,t={}){const s=oS(new Uint8Array(e));if(globalThis.__hyperthreeKtx2Transcode){const r=await globalThis.__hyperthreeKtx2Transcode(new Uint8Array(e),this.workerConfig||t||{});if(r){r.data.format=this.constructor.EngineFormat[r.data.format];r.data.type=this.constructor.EngineType[r.data.type];return this._createTextureFrom(r,s)}}const i="#;
        if normalized.contains(minified_ktx_from)
            && !normalized
                .contains("__hyperthreeKtx2Transcode(new Uint8Array(e),this.workerConfig||t||{})")
        {
            normalized = normalized.replace(minified_ktx_from, minified_ktx_to);
            boa_changed = true;
        }
    }
    if is_draco_loader_bundle {
        for (from, to) in [
            (
                "preload() {\n\n\t\tthis._initDecoder();",
                "preload() {\n\n\t\tif ( globalThis.__hyperthreeDecodeDraco ) return this;\n\n\t\tthis._initDecoder();",
            ),
            (
                "preload() {\n      this._initDecoder();",
                "preload() {\n      if ( globalThis.__hyperthreeDecodeDraco ) return this;\n      this._initDecoder();",
            ),
            (
                "preload(){return this._initDecoder(),this}",
                "preload(){if(globalThis.__hyperthreeDecodeDraco)return this;return this._initDecoder(),this}",
            ),
        ] {
            if normalized.contains(from) {
                normalized = normalized.replace(from, to);
                boa_changed = true;
            }
        }
        let from = r#"decodeDracoFile( buffer, callback, attributeIDs, attributeTypes, vertexColorSpace = LinearSRGBColorSpace, onError = () => {} ) {"#;
        let to = r#"decodeDracoFile( buffer, callback, attributeIDs, attributeTypes, vertexColorSpace = LinearSRGBColorSpace, onError = () => {} ) {

		if ( globalThis.__hyperthreeDecodeDraco ) {

			try {

				globalThis.__hyperthreeDracoNativeCalls = ( globalThis.__hyperthreeDracoNativeCalls || 0 ) + 1;
				const nativeResult = globalThis.__hyperthreeDecodeDraco( new Uint8Array( buffer ) );
				const requestedTypes = attributeTypes || this.defaultAttributeTypes;
				const attributes = [];

				for ( const nativeAttribute of nativeResult.attributes ) {

					const typeName = requestedTypes[ nativeAttribute.name ] || 'Float32Array';
					const ArrayType = globalThis[ typeName ] || Float32Array;
					const values = new Float32Array( nativeAttribute.data );
					const typedValues = ArrayType === Float32Array ? values : new ArrayType( values );
					attributes.push( { name: nativeAttribute.name, array: typedValues, itemSize: nativeAttribute.itemSize, vertexColorSpace: vertexColorSpace } );

				}

				const geometry = this._createGeometry( { index: { array: new Uint32Array( nativeResult.index ) }, attributes: attributes } );
				return Promise.resolve( geometry ).then( callback ).catch( onError );

			} catch ( error ) {

				return Promise.reject( error ).catch( onError );

			}

		}
"#;
        if normalized.contains(from) {
            normalized = normalized.replace(from, to);
            boa_changed = true;
        }
        let bundled_from = r#"decodeDracoFile(buffer2, callback, attributeIDs, attributeTypes, vertexColorSpace = LinearSRGBColorSpace, onError = () => {
    }) {"#;
        let bundled_hook = r#"
      if (globalThis.__hyperthreeDecodeDraco) {
        try {
          globalThis.__hyperthreeDracoNativeCalls = (globalThis.__hyperthreeDracoNativeCalls || 0) + 1;
          const nativeResult = globalThis.__hyperthreeDecodeDraco(new Uint8Array(buffer2));
          const requestedTypes = attributeTypes || this.defaultAttributeTypes;
          const attributes = [];
          for (const nativeAttribute of nativeResult.attributes) {
            const typeName = requestedTypes[nativeAttribute.name] || 'Float32Array';
            const ArrayType = globalThis[typeName] || Float32Array;
            const values = new Float32Array(nativeAttribute.data);
            const typedValues = ArrayType === Float32Array ? values : new ArrayType(values);
            attributes.push({name: nativeAttribute.name, array: typedValues, itemSize: nativeAttribute.itemSize, vertexColorSpace});
          }
          const geometry = this._createGeometry({index: {array: new Uint32Array(nativeResult.index)}, attributes});
          return Promise.resolve(geometry).then(callback).catch(onError);
        } catch (error) {
          return Promise.reject(error).catch(onError);
        }
      }
"#;
        if normalized.contains(bundled_from)
            && !normalized.contains(
                "const nativeResult = globalThis.__hyperthreeDecodeDraco(new Uint8Array(buffer2))",
            )
        {
            normalized = normalized.replace(bundled_from, &format!("{bundled_from}{bundled_hook}"));
            boa_changed = true;
        }
        let minified_from = "decodeDracoFile(e,t,s,i,n=ot,r=()=>{}){";
        let minified_hook = r#"if(globalThis.__hyperthreeDecodeDraco){try{globalThis.__hyperthreeDracoNativeCalls=(globalThis.__hyperthreeDracoNativeCalls||0)+1;const o=globalThis.__hyperthreeDecodeDraco(new Uint8Array(e)),a=i||this.defaultAttributeTypes,c=[];for(const e of o.attributes){const A=a[e.name]||'Float32Array',p=globalThis[A]||Float32Array,f=new Float32Array(e.data),g=p===Float32Array?f:new p(f);c.push({name:e.name,array:g,itemSize:e.itemSize,vertexColorSpace:n})}const h=this._createGeometry({index:{array:new Uint32Array(o.index)},attributes:c});return Promise.resolve(h).then(t).catch(r)}catch(e){return Promise.reject(e).catch(r)}}"#;
        if normalized.contains(minified_from)
            && !normalized.contains("new Uint8Array(e),a=i||this.defaultAttributeTypes")
        {
            normalized =
                normalized.replace(minified_from, &format!("{minified_from}{minified_hook}"));
            boa_changed = true;
        }
        let minified_from_alt = "decodeDracoFile(e,t,s,i,n=at,r=()=>{}){";
        if normalized.contains(minified_from_alt)
            && !normalized.contains("new Uint8Array(e),a=i||this.defaultAttributeTypes")
        {
            normalized = normalized.replace(
                minified_from_alt,
                &format!("{minified_from_alt}{minified_hook}"),
            );
            boa_changed = true;
        }
    }
    if is_gltf_loader_bundle
        || is_ktx2_loader_bundle
        || is_draco_loader_bundle
        || source.contains("decodeAudioData")
    {
        boa_changed |= normalize_boa_class_method_bindings(&mut normalized);
    }
    if source.contains("decodeAudioData") {
        boa_changed |= normalize_boa_audio_loader_method(&mut normalized);
    }
    if boa_changed || normalized != source {
        std::borrow::Cow::Owned(normalized)
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

/// Boa 0.21.1 can allocate zero lexical slots for a class method and then emit
/// `PutLexicalValue` for a local `const`/`let`, which panics at runtime. Three.js
/// loaders contain methods with local bindings, notably AudioLoader.load().
/// Restrict the compatibility rewrite to methods in class bodies; game code
/// and ordinary functions retain normal lexical semantics.
fn normalize_boa_class_method_bindings(source: &mut String) -> bool {
    let ranges = class_method_ranges(source);
    if ranges.is_empty() {
        return false;
    }

    let bytes = source.as_bytes();
    let mut replacements = Vec::new();
    for (start, end) in ranges {
        let mut cursor = start;
        while cursor < end {
            if let Some((next, token)) = next_code_identifier(bytes, cursor, end) {
                cursor = next;
                if token == "const" || token == "let" {
                    let token_start = next - token.len();
                    replacements.push((token_start, next, "var"));
                }
            } else {
                break;
            }
        }
    }

    if replacements.is_empty() {
        return false;
    }

    for (start, end, replacement) in replacements.into_iter().rev() {
        source.replace_range(start..end, replacement);
    }
    true
}

/// Boa 0.21 also loses the lexical slot for the module-level private context
/// variable emitted by Three.js' AudioContext helper when the source is bundled
/// into one script. Convert only that declaration to a function-safe `var`; the
/// public AudioContext object and all other game lexical bindings remain intact.
fn normalize_boa_audio_context_binding(source: &mut String) -> bool {
    let Some(context_marker) = source.rfind("getContext(){return ") else {
        return false;
    };
    let Some(class_start) = source[..context_marker].rfind("class ") else {
        return false;
    };
    let Some(let_start) = source[..class_start].rfind("let ") else {
        return false;
    };
    let declaration = &source[let_start..class_start];
    let Some(semi) = declaration.find(';') else {
        return false;
    };
    if !declaration[semi + 1..].trim().is_empty() {
        return false;
    }
    source.replace_range(let_start..let_start + 3, "var");
    true
}

/// The bundled AudioLoader method is not reliably assigned a lexical
/// environment by Boa even when its surrounding class is detected by the
/// general class scanner. Normalize only that method, identified by the
/// `decodeAudioData` call it owns.
fn normalize_boa_audio_loader_method(source: &mut String) -> bool {
    let Some(marker) = source.find("decodeAudioData") else {
        return false;
    };
    let Some(class_start) = source[..marker].rfind("class ") else {
        return false;
    };
    let bytes = source.as_bytes();
    let Some(class_open) = next_code_byte(bytes, class_start + 6, bytes.len(), b'{') else {
        return false;
    };
    let Some(class_close) = matching_delimiter(bytes, class_open, b'{', b'}') else {
        return false;
    };
    let mut cursor = class_open + 1;
    while cursor < class_close {
        let Some((identifier_end, identifier)) = next_code_identifier(bytes, cursor, class_close)
        else {
            break;
        };
        cursor = identifier_end;
        if identifier != "load" {
            continue;
        }
        let Some(parameter_open) = next_code_byte(bytes, cursor, class_close, b'(') else {
            continue;
        };
        let Some(parameter_close) = matching_delimiter(bytes, parameter_open, b'(', b')') else {
            continue;
        };
        let Some(body_open) = next_code_byte(bytes, parameter_close + 1, class_close, b'{') else {
            continue;
        };
        let Some(body_close) = matching_delimiter(bytes, body_open, b'{', b'}') else {
            continue;
        };
        let mut replacements = Vec::new();
        let mut body_cursor = body_open + 1;
        while body_cursor < body_close {
            let Some((next, token)) = next_code_identifier(bytes, body_cursor, body_close) else {
                break;
            };
            body_cursor = next;
            if token == "const" || token == "let" {
                replacements.push((next - token.len(), next));
            }
        }
        for (start, end) in replacements.into_iter().rev() {
            source.replace_range(start..end, "var");
        }
        return true;
    }
    false
}

fn class_method_ranges(source: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while let Some((class_end, token)) = next_code_identifier(bytes, cursor, bytes.len()) {
        cursor = class_end;
        if token != "class" {
            continue;
        }
        let Some(class_open) = next_code_byte(bytes, cursor, bytes.len(), b'{') else {
            continue;
        };
        let Some(class_close) = matching_delimiter(bytes, class_open, b'{', b'}') else {
            continue;
        };
        let mut body_cursor = class_open + 1;
        while body_cursor < class_close {
            let Some((identifier_end, _identifier)) =
                next_code_identifier(bytes, body_cursor, class_close)
            else {
                break;
            };
            body_cursor = identifier_end;
            let Some(parameter_open) = next_code_byte(bytes, body_cursor, class_close, b'(') else {
                continue;
            };
            let Some(parameter_close) = matching_delimiter(bytes, parameter_open, b'(', b')')
            else {
                continue;
            };
            let Some(body_open) = next_code_byte(bytes, parameter_close + 1, class_close, b'{')
            else {
                continue;
            };
            let Some(body_close) = matching_delimiter(bytes, body_open, b'{', b'}') else {
                continue;
            };
            ranges.push((body_open + 1, body_close));
            body_cursor = body_close + 1;
        }
        cursor = class_close + 1;
    }
    ranges
}

fn next_code_byte(bytes: &[u8], mut cursor: usize, end: usize, wanted: u8) -> Option<usize> {
    while cursor < end {
        if is_ignored_byte(bytes, &mut cursor, end) {
            continue;
        }
        if bytes[cursor] == wanted {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

fn next_code_identifier(bytes: &[u8], mut cursor: usize, end: usize) -> Option<(usize, &str)> {
    while cursor < end {
        if is_ignored_byte(bytes, &mut cursor, end) {
            continue;
        }
        if is_identifier_start(bytes[cursor]) {
            let start = cursor;
            cursor += 1;
            while cursor < end && is_identifier_part(bytes[cursor]) {
                cursor += 1;
            }
            return std::str::from_utf8(&bytes[start..cursor])
                .ok()
                .map(|identifier| (cursor, identifier));
        }
        cursor += 1;
    }
    None
}

fn matching_delimiter(bytes: &[u8], open: usize, opening: u8, closing: u8) -> Option<usize> {
    let mut cursor = open;
    let mut depth = 0usize;
    while cursor < bytes.len() {
        if is_ignored_byte(bytes, &mut cursor, bytes.len()) {
            continue;
        }
        match bytes[cursor] {
            value if value == opening => depth += 1,
            value if value == closing => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn is_ignored_byte(bytes: &[u8], cursor: &mut usize, end: usize) -> bool {
    if *cursor >= end {
        return false;
    }
    match bytes[*cursor] {
        b'\'' | b'"' | b'`' => {
            let quote = bytes[*cursor];
            *cursor += 1;
            while *cursor < end {
                if bytes[*cursor] == b'\\' {
                    *cursor = (*cursor + 2).min(end);
                } else if bytes[*cursor] == quote {
                    *cursor += 1;
                    break;
                } else {
                    *cursor += 1;
                }
            }
            true
        }
        b'/' if *cursor + 1 < end && bytes[*cursor + 1] == b'/' => {
            *cursor += 2;
            while *cursor < end && bytes[*cursor] != b'\n' {
                *cursor += 1;
            }
            true
        }
        b'/' if *cursor + 1 < end && bytes[*cursor + 1] == b'*' => {
            *cursor += 2;
            while *cursor + 1 < end && !(bytes[*cursor] == b'*' && bytes[*cursor + 1] == b'/') {
                *cursor += 1;
            }
            *cursor = (*cursor + 2).min(end);
            true
        }
        _ => false,
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_identifier_part(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
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

fn optional_number_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
    default: f64,
) -> JsResult<f64> {
    let value = args.get_or_undefined(index);
    if value.is_undefined() || value.is_null() {
        return Ok(default);
    }
    let value = number_arg(args, index, context)?;
    if value < 0.0 {
        return Err(JsNativeError::range()
            .with_message("audio time values must be non-negative")
            .into());
    }
    Ok(value)
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

fn primitive_kind_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<GeometryKind> {
    let value = number_arg(args, index, context)?;
    if value.fract() != 0.0 {
        return Err(JsNativeError::range()
            .with_message("primitive kind must be an integer")
            .into());
    }
    match value as i32 {
        0 => Ok(GeometryKind::Cube),
        1 => Ok(GeometryKind::Plane),
        2 => Ok(GeometryKind::Sphere),
        _ => Err(JsNativeError::range()
            .with_message("unknown native primitive kind")
            .into()),
    }
}

fn optional_texture_id_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<Option<u64>> {
    let value = args.get_or_undefined(index);
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    let value = number_arg(args, index, context)?;
    if value < 0.0 {
        return Ok(None);
    }
    if value.fract() != 0.0 || value > u64::MAX as f64 {
        return Err(JsNativeError::range()
            .with_message("texture id must be a non-negative integer")
            .into());
    }
    Ok(Some(value as u64))
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

fn byte_array_value(value: &JsValue, context: &mut Context) -> JsResult<Vec<u8>> {
    let object = value
        .to_object(context)
        .map_err(|_| JsNativeError::typ().with_message("image bytes must be array-like"))?;
    let length = object
        .get(js_string!("length"), context)?
        .to_length(context)
        .map_err(|_| JsNativeError::typ().with_message("image byte length is invalid"))?;
    if length > 64 * 1024 * 1024 {
        return Err(JsNativeError::range()
            .with_message("image is too large")
            .into());
    }
    let mut bytes = Vec::with_capacity(length as usize);
    for index in 0..length as usize {
        let byte = object.get(index, context)?.to_number(context)?;
        if !byte.is_finite() || !(0.0..=255.0).contains(&byte) || byte.fract() != 0.0 {
            return Err(JsNativeError::range()
                .with_message("image byte data is invalid")
                .into());
        }
        bytes.push(byte as u8);
    }
    Ok(bytes)
}

fn raw_ktx2_result(source: &[u8], context: &mut Context) -> JsResult<JsValue> {
    if source.len() < 104 {
        return Ok(JsValue::null());
    }
    let vk_format = u32::from_le_bytes(source[12..16].try_into().unwrap());
    let (format, type_name) = match vk_format {
        134 => ("RGBA_S3TC_DXT1_Format", "UnsignedByteType"),
        137 => ("RGBA_S3TC_DXT5_Format", "UnsignedByteType"),
        145 => ("RGBA_BPTC_Format", "UnsignedByteType"),
        151 => ("RGBA_ETC2_EAC_Format", "UnsignedByteType"),
        _ => return Ok(JsValue::null()),
    };
    let width = u32::from_le_bytes(source[20..24].try_into().unwrap());
    let height = u32::from_le_bytes(source[24..28].try_into().unwrap());
    let level_count = u32::from_le_bytes(source[36..40].try_into().unwrap()).max(1);
    let face_count = u32::from_le_bytes(source[44..48].try_into().unwrap()).max(1);
    let mut faces = Vec::with_capacity(face_count as usize);
    for face in 0..face_count {
        let mut mipmaps = Vec::with_capacity(level_count as usize);
        for level in 0..level_count {
            let entry = 80usize
                .checked_add(level as usize * 24)
                .ok_or_else(|| JsNativeError::range().with_message("KTX2 level index overflow"))?;
            if entry + 24 > source.len() {
                return Err(JsNativeError::range()
                    .with_message("KTX2 level index is outside the source")
                    .into());
            }
            let offset = u64::from_le_bytes(source[entry..entry + 8].try_into().unwrap()) as usize;
            let length =
                u64::from_le_bytes(source[entry + 8..entry + 16].try_into().unwrap()) as usize;
            let face_length = length / face_count as usize;
            let face_offset = offset
                .checked_add(face as usize * face_length)
                .ok_or_else(|| JsNativeError::range().with_message("KTX2 face offset overflow"))?;
            let end = face_offset
                .checked_add(face_length)
                .ok_or_else(|| JsNativeError::range().with_message("KTX2 face range overflow"))?;
            if end > source.len() {
                return Err(JsNativeError::range()
                    .with_message("KTX2 level payload is outside the source")
                    .into());
            }
            let data = JsArrayBuffer::from_byte_block(
                AlignedVec::from_iter(0, source[face_offset..end].to_vec()),
                context,
            )
            .map_err(|error| {
                JsNativeError::error()
                    .with_message(format!("failed to create raw KTX2 buffer: {error}"))
            })?;
            let mipmap = JsObject::with_object_proto(context.intrinsics());
            mipmap.set(js_string!("data"), data, false, context)?;
            mipmap.set(
                js_string!("width"),
                JsValue::from((width >> level).max(1) as f64),
                false,
                context,
            )?;
            mipmap.set(
                js_string!("height"),
                JsValue::from((height >> level).max(1) as f64),
                false,
                context,
            )?;
            mipmaps.push(mipmap.into());
        }
        let face_value = JsObject::with_object_proto(context.intrinsics());
        face_value.set(
            js_string!("mipmaps"),
            JsArray::from_iter(mipmaps, context),
            false,
            context,
        )?;
        faces.push(face_value.into());
    }
    let data = JsObject::with_object_proto(context.intrinsics());
    data.set(
        js_string!("faces"),
        JsArray::from_iter(faces, context),
        false,
        context,
    )?;
    data.set(
        js_string!("width"),
        JsValue::from(width as f64),
        false,
        context,
    )?;
    data.set(
        js_string!("height"),
        JsValue::from(height as f64),
        false,
        context,
    )?;
    data.set(js_string!("format"), js_string!(format), false, context)?;
    data.set(js_string!("type"), js_string!(type_name), false, context)?;
    data.set(js_string!("dfdFlags"), JsValue::from(0), false, context)?;
    let result = JsObject::with_object_proto(context.intrinsics());
    result.set(js_string!("type"), js_string!("transcode"), false, context)?;
    result.set(js_string!("data"), data, false, context)?;
    Ok(result.into())
}

fn js_bool_property(value: &JsValue, property: &str, context: &mut Context) -> JsResult<bool> {
    if value.is_null() || value.is_undefined() {
        return Ok(false);
    }
    let object = value.to_object(context).map_err(|_| {
        JsNativeError::typ().with_message("KTX2Loader worker config must be an object")
    })?;
    Ok(object.get(js_string!(property), context)?.to_boolean())
}

fn choose_basis_target(
    transcoder: &Transcoder<'_>,
    has_alpha: bool,
    astc_supported: bool,
    bptc_supported: bool,
    dxt_supported: bool,
    etc2_supported: bool,
) -> std::result::Result<(TargetFormat, &'static str, &'static str), String> {
    let is_hdr = matches!(
        transcoder.source_format(),
        SourceFormat::UastcHdr4x4 | SourceFormat::AstcHdr6x6 | SourceFormat::UastcHdr6x6
    );
    let candidates = if is_hdr {
        let mut candidates = Vec::new();
        if bptc_supported {
            candidates.push((
                TargetFormat::Bc6h,
                "RGB_BPTC_UNSIGNED_Format",
                "HalfFloatType",
            ));
        }
        candidates.push((TargetFormat::RgbaHalf, "RGBAFormat", "HalfFloatType"));
        candidates
    } else {
        let mut candidates = Vec::new();
        if astc_supported {
            candidates.push((
                TargetFormat::Astc4x4Rgba,
                "RGBA_ASTC_4x4_Format",
                "UnsignedByteType",
            ));
        }
        if bptc_supported {
            candidates.push((
                TargetFormat::Bc7Rgba,
                "RGBA_BPTC_Format",
                "UnsignedByteType",
            ));
        }
        if dxt_supported {
            candidates.push(if has_alpha {
                (
                    TargetFormat::Bc3Rgba,
                    "RGBA_S3TC_DXT5_Format",
                    "UnsignedByteType",
                )
            } else {
                (
                    TargetFormat::Bc1Rgb,
                    "RGBA_S3TC_DXT1_Format",
                    "UnsignedByteType",
                )
            });
        }
        if etc2_supported {
            candidates.push((
                TargetFormat::Etc2Rgba,
                "RGBA_ETC2_EAC_Format",
                "UnsignedByteType",
            ));
        }
        candidates.push((TargetFormat::Rgba32, "RGBAFormat", "UnsignedByteType"));
        candidates
    };

    candidates
        .into_iter()
        .find(|(target, _, _)| transcoder.supports(*target))
        .ok_or_else(|| {
            format!(
                "Basis/KTX2 texture source {:?} has no supported native transcode target",
                transcoder.source_format()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{normalize_three_compatibility_source, JsRuntime};
    use crate::bridge::{NativeInputState, NativeRenderState};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn normalizes_three_chain_map_empty_key_lookup() {
        let minified = "class ChainMap{constructor(){this.weakMap=new WeakMap}get(e){let t=this.weakMap;return t.get(e[e.length-1])}}";
        let normalized = normalize_three_compatibility_source(minified);
        assert!(normalized.contains("get(e){if(e.length===0)return;let t=this.weakMap;"));

        let readable = "class ChainMap {\n\tget( keys ) {\n\n\t\tlet map = this.weakMap;\n\t}";
        let normalized = normalize_three_compatibility_source(readable);
        assert!(normalized.contains("if ( keys.length === 0 ) return undefined;"));
    }

    #[test]
    fn leaves_non_three_source_unchanged() {
        let source = "const value = 42;";
        assert!(matches!(
            normalize_three_compatibility_source(source),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn injects_native_meshopt_decoder_into_three_loader() {
        let source =
            "/* GLTFLoader */ class GLTFLoader{constructor(){this.meshoptDecoder = null;}}";
        let normalized = normalize_three_compatibility_source(source);
        assert!(normalized
            .contains("this.meshoptDecoder = globalThis.__hyperthreeMeshoptDecoder || null;"));
        let minified =
            "/* GLTFLoader */ class GLTFLoader{constructor(){this.meshoptDecoder=null;}}";
        let normalized = normalize_three_compatibility_source(minified);
        assert!(
            normalized.contains("this.meshoptDecoder=globalThis.__hyperthreeMeshoptDecoder||null;")
        );
        let bundled = "/* GLTFLoader */ class GLTFLoader{constructor(){super(),this.meshoptDecoder=null,this.pluginCallbacks=[]}}";
        let normalized = normalize_three_compatibility_source(bundled);
        assert!(
            normalized.contains("this.meshoptDecoder=globalThis.__hyperthreeMeshoptDecoder||null,")
        );
    }

    #[test]
    fn injects_native_ktx2_transcoder_into_ktx2_loader() {
        let source = "/* KTX2Loader */ class KTX2Loader{async _createTexture(buffer){const container = read( new Uint8Array( buffer ) ); return container;}}";
        let normalized = normalize_three_compatibility_source(source);
        assert!(normalized.contains("__hyperthreeKtx2Transcode"));
        assert!(normalized.contains("new Uint8Array( buffer )"));
        assert!(normalized.contains("KTX2Loader.EngineFormat"));
        let minified = "/* KTX2Loader */ async _createTexture(e,t={}){const s=oS(new Uint8Array(e)),i=1; return i;}";
        let normalized = normalize_three_compatibility_source(minified);
        assert!(normalized.contains("__hyperthreeKtx2Transcode"));
    }

    #[test]
    fn normalizes_boa_audio_context_private_lexical_binding() {
        let source = "let na;class gb{static getContext(){return na===void 0&&(na=new(window.AudioContext||window.webkitAudioContext)),na}}function decodeAudioData(){}";
        let normalized = normalize_three_compatibility_source(source);
        assert!(normalized.contains("var na;"));
    }

    #[test]
    fn normalizes_boa_audio_loader_method_lexicals() {
        let source = "class Lb extends jc{constructor(e){super(e)}load(e,t){const n=this;const buffer=e.slice(0);Mb.getContext().decodeAudioData(buffer,t)}}";
        let normalized = normalize_three_compatibility_source(source);
        assert!(normalized.contains("load(e,t){var n=this;var buffer=e.slice(0);"));
    }

    #[test]
    fn injects_native_draco_decoder_and_skips_worker_preload() {
        let source = r#"/* DRACOLoader */ class DRACOLoader{preload() {

		this._initDecoder();

		return this;
	}
	decodeDracoFile( buffer, callback, attributeIDs, attributeTypes, vertexColorSpace = LinearSRGBColorSpace, onError = () => {} ) { return this.decodeGeometry( buffer, {} ); }}"#;
        let normalized = normalize_three_compatibility_source(source);
        assert!(normalized.contains("__hyperthreeDecodeDraco"));
        assert!(normalized.contains("__hyperthreeDracoNativeCalls"));
        assert!(normalized.contains("if ( globalThis.__hyperthreeDecodeDraco ) return this;"));
        let minified = "/* DRACOLoader */ preload(){return this._initDecoder(),this} decodeDracoFile(e,t,s,i,n=at,r=()=>{}){}";
        let normalized = normalize_three_compatibility_source(minified);
        assert!(normalized.contains("__hyperthreeDecodeDraco"));
    }

    #[cfg(any())]
    #[test]
    fn native_meshopt_binding_decodes_a_typed_array() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                "globalThis.__meshoptProbe = __hyperthreeDecodeMeshopt(new Uint8Array([160,0,0,1,60,0,0,0,255,255,1,60,0,0,0,126,125,0,0,1,12,0,0,0,255,1,12,0,0,0,126,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]), 3, 12, 'ATTRIBUTES', 'NONE');",
            )
            .unwrap();
    }

    #[test]
    fn native_meshopt_binding_decodes_fetched_bytes() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.__meshoptProbe = false;
                fetch('data:application/octet-stream;base64,oAAAATwAAAD//wE8AAAAfn0AAAEMAAAA/wEMAAAAfgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==')
                  .then((response) => response.arrayBuffer())
                  .then((buffer) => {
                    const decoded = __hyperthreeDecodeMeshopt(new Uint8Array(buffer), 3, 12, 'ATTRIBUTES', 'NONE');
                    globalThis.__meshoptProbe = decoded.byteLength === 36;
                  });
                "#,
            )
            .unwrap();
        runtime
            .execute_source("if (globalThis.__meshoptProbe !== true) throw new Error('meshopt binding probe failed');")
            .unwrap();
    }

    #[test]
    fn native_basis_ktx2_binding_selects_rgba32_and_bc7() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.__basisProbe = false;
                fetch('data:application/octet-stream;base64,q0tUWCAyMLsNChoKAAAAAAEAAAAIAAAACAAAAAAAAAAAAAAAAQAAAAEAAAABAAAAaAAAADwAAACkAAAARAAAAOgAAAAAAAAAjAAAAAAAAAB0AQAAAAAAAAMAAAAAAAAAAAAAAAAAAAA8AAAAAAAAAAIAOACjAQIAAwMAAAgIAAAAAAAAAAA/AAAAAAAAAAAA/////0AAPw8AAAAAAAAAAP////9AAAAAS1RYd3JpdGVyAGt0eCBjcmVhdGUgdjUuMC5fX2RlZmF1bHRfXyAvIGxpYmt4IHY1LjAuX19kZWZhdWx0X18AAQIAAgAtAAAACQAAAC4AAAAAAAAAAAAAAAAAAAABAAAAAQAAAAIAAAABwAQAAAAAAAACBJgbIAAAAAjDNpE+kQBgAgAAAAAAAIEATAEQAAAAACBZwD2sqqqqUlVVVQUUwEQAAAAAAAASQQCYAAAAAAAAQBgCogQMAAAAg3Z7SQSiIABMAAgAAAAAIAIBBkwO')
                  .then((response) => response.arrayBuffer())
                  .then((buffer) => {
                    const result = __hyperthreeTranscodeKtx2(new Uint8Array(buffer), {});
                    const mipmap = result.data.faces[0].mipmaps[0];
                    const compressed = __hyperthreeTranscodeKtx2(new Uint8Array(buffer), { bptcSupported: true });
                    const compressedMipmap = compressed.data.faces[0].mipmaps[0];
                    globalThis.__basisProbe = result.type === 'transcode' &&
                      result.data.format === 'RGBAFormat' &&
                      mipmap.width === 8 && mipmap.height === 8 &&
                      mipmap.data.byteLength === 256 &&
                      compressed.data.format === 'RGBA_BPTC_Format' &&
                      compressedMipmap.data.byteLength === 64;
                  });
                "#,
            )
            .unwrap();
        runtime
            .execute_source("if (globalThis.__basisProbe !== true) throw new Error('Basis KTX2 binding probe failed');")
            .unwrap();
    }

    #[test]
    fn native_uastc_ktx2_binding_transcodes_raw_uastc_blocks() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.__uastcProbe = false;
                fetch('data:application/octet-stream;base64,q0tUWCAyMLsNChoKAAAAAAEAAAAEAAAABAAAAAAAAAAAAAAAAQAAAAEAAAAAAAAAaAAAACwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACYAAAAAAAAABAAAAAAAAAAEAAAAAAAAAAsAAAAAAAAAAIAKACmAQIAAwMAABAAAAAAAAAAAAB/AAAAAAAAAAAA/////wAAAAD3HwjkHwAAAAAAAAAAAAAA')
                  .then((response) => response.arrayBuffer())
                  .then((buffer) => {
                    const rgba = __hyperthreeTranscodeKtx2(new Uint8Array(buffer), {});
                    const bc7 = __hyperthreeTranscodeKtx2(new Uint8Array(buffer), { bptcSupported: true });
                    const rgbaMipmap = rgba.data.faces[0].mipmaps[0];
                    const bc7Mipmap = bc7.data.faces[0].mipmaps[0];
                    globalThis.__uastcProbe = rgba.data.format === 'RGBAFormat' &&
                      rgbaMipmap.width === 4 && rgbaMipmap.height === 4 &&
                      rgbaMipmap.data.byteLength === 64 &&
                      bc7.data.format === 'RGBA_BPTC_Format' &&
                      bc7Mipmap.data.byteLength === 16;
                  });
                "#,
            )
            .unwrap();
        runtime
            .execute_source("if (globalThis.__uastcProbe !== true) throw new Error('UASTC KTX2 binding probe failed');")
            .unwrap();
    }

    #[test]
    fn native_audio_context_decodes_standard_audio_buffer_shape() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.__audioProbe = false;
                const context = new AudioContext();
                fetch('data:audio/wav;base64,UklGRiwAAABXQVZFZm10IBAAAAABAAEACAAAABAAAAACABAAZGF0YQgAAAAAAAAgAOAAAA==')
                  .then((response) => response.arrayBuffer())
                  .then((buffer) => context.decodeAudioData(buffer))
                  .then((audioBuffer) => {
                    const gain = context.createGain();
                    const source = context.createBufferSource();
                    source.buffer = audioBuffer;
                    source.connect(gain);
                    gain.connect(context.destination);
                    gain.gain.setValueAtTime(0.5, context.currentTime);
                    globalThis.__audioProbe = audioBuffer.sampleRate === 8 &&
                      audioBuffer.length === 4 && audioBuffer.numberOfChannels === 1 &&
                      Math.abs(audioBuffer.getChannelData(0)[1] - 0.25) < 0.01 &&
                      typeof source.start === 'function' && typeof source.stop === 'function';
                  });
                "#,
            )
            .unwrap();
        runtime
            .execute_source("if (globalThis.__audioProbe !== true) throw new Error('AudioContext compatibility probe failed');")
            .unwrap();
    }

    #[test]
    fn blob_file_object_url_round_trips_through_fetch() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.__blobUrlProbe = false;
                const file = new File([new Uint8Array([7, 11, 13])], 'probe.bin', {
                  type: 'application/octet-stream',
                  lastModified: 123,
                });
                const url = URL.createObjectURL(file);
                fetch(url)
                  .then((response) => response.arrayBuffer())
                  .then((buffer) => {
                    const bytes = new Uint8Array(buffer);
                    globalThis.__blobUrlProbe = file.name === 'probe.bin' &&
                      file.type === 'application/octet-stream' &&
                      file.lastModified === 123 &&
                      url.startsWith('blob:hyperthree/') &&
                      bytes.length === 3 && bytes[0] === 7 && bytes[1] === 11 && bytes[2] === 13;
                    URL.revokeObjectURL(url);
                  });
                "#,
            )
            .unwrap();
        runtime
            .execute_source(
                "if (globalThis.__blobUrlProbe !== true) throw new Error('Blob/File object URL probe failed');",
            )
            .unwrap();
    }

    #[test]
    fn local_storage_persists_across_runtime_sessions_inside_project_root() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-js-storage-test-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let mut runtime = JsRuntime::new(render_state.clone(), input_state.clone(), &root).unwrap();
        runtime
            .execute_source(
                r#"
                localStorage.setItem('score', 42);
                localStorage.setItem('pilot', 'Ada');
                sessionStorage.setItem('temporary', 'yes');
                const keys = [localStorage.key(0), localStorage.key(1)];
                if (localStorage.length !== 2 || localStorage.getItem('score') !== '42' ||
                    !keys.includes('score') || !keys.includes('pilot') || sessionStorage.getItem('temporary') !== 'yes') {
                  throw new Error('storage API probe failed');
                }
                "#,
            )
            .unwrap();
        drop(runtime);
        let mut restored = JsRuntime::new(render_state, input_state, &root).unwrap();
        restored
            .execute_source(
                "if (localStorage.length !== 2 || localStorage.getItem('score') !== '42' || localStorage.getItem('pilot') !== 'Ada' || sessionStorage.getItem('temporary') !== null) throw new Error('storage persistence probe failed');",
            )
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normalizes_boa_class_method_lexicals_only() {
        let source = "/* GLTFLoader */ class Example{constructor(){const value=1;let other=2;this.value=value+other;}method(){const untouched=3;return untouched;}} let of='';";
        let normalized = normalize_three_compatibility_source(source);
        assert!(normalized.contains("constructor(){var value=1;var other=2;"));
        assert!(normalized.contains("method(){var untouched=3;"));
        assert!(normalized.contains("var of='';"));
    }

    #[test]
    fn executes_loader_style_derived_audio_constructor() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                "/* GLTFLoader */ class Loader{constructor(e){this.manager=e;}} class AudioLoader extends Loader{constructor(e){super(e)}}; new AudioLoader(1);",
            )
            .unwrap();
    }

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
        assert_eq!(snapshot.cubes[0].model_matrix.unwrap()[0][0], 2.0);
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
                      material: {
                        color: { r: 0.4, g: 0.5, b: 0.6 },
                        opacity: 1,
                        isMeshStandardMaterial: true,
                        metalness: 0.8,
                        roughness: 0.2,
                        emissive: { r: 0.03, g: 0.02, b: 0.01 },
                      },
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
        assert_eq!(snapshot.custom_meshes[0].material.metallic, 0.8);
        assert_eq!(snapshot.custom_meshes[0].material.roughness, 0.2);
        assert_eq!(
            snapshot.custom_meshes[0].material.emissive,
            [0.03, 0.02, 0.01]
        );
        let registry = snapshot.geometry_registry.lock().unwrap();
        let geometry = registry.get(42).unwrap();
        assert_eq!(geometry.positions.len(), 3);
        assert_eq!(geometry.indices, [0, 1, 2]);
    }

    #[test]
    fn three_scene_sync_converts_points_to_native_particles() {
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
                      isPoints: true,
                      position: { x: 2, y: 0, z: 0 },
                      geometry: {
                        attributes: {
                          position: { array: new Float32Array([0, 0, 0, 1, 0, 0]) },
                        },
                      },
                      material: {
                        size: 0.25,
                        color: { r: 1, g: 0.2, b: 0.1 },
                        opacity: 0.75,
                      },
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
        assert_eq!(snapshot.particles.len(), 2);
        assert_eq!(snapshot.particles[0].position, [2.0, 0.0, 0.0]);
        assert_eq!(snapshot.particles[1].position, [3.0, 0.0, 0.0]);
        assert_eq!(snapshot.particles[0].size, 0.25);
    }

    #[test]
    fn animated_matrix_world_is_forwarded_each_frame() {
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
                const object = {
                  visible: true,
                  isMesh: true,
                  geometry: { type: "BoxGeometry" },
                  position: { x: 0, y: 0, z: 0 },
                  scale: { x: 1, y: 1, z: 1 },
                  rotation: { y: 0 },
                  matrixWorld: { elements: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] },
                  material: { color: { r: 1, g: 1, b: 1 }, opacity: 1 },
                };
                const scene = {
                  updateMatrixWorld() {},
                  traverse(callback) { callback(object); },
                };
                let x = 0;
                globalThis.HyperThreeGame = {
                  update() {
                    x += 1;
                    object.matrixWorld.elements[12] = x;
                    HyperThreeNative.syncThreeScene(scene);
                  },
                };
                "#,
            )
            .unwrap();
        runtime.execute_frame(1.0 / 60.0).unwrap();
        assert_eq!(
            render_state.lock().unwrap().snapshot().cubes[0].position,
            [1.0, 0.0, 0.0]
        );
        runtime.execute_frame(1.0 / 60.0).unwrap();
        assert_eq!(
            render_state.lock().unwrap().snapshot().cubes[0].position,
            [2.0, 0.0, 0.0]
        );
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
    fn fetch_returns_project_relative_asset_arraybuffer() {
        let render_state = NativeRenderState::shared();
        let input_state = NativeInputState::shared();
        let root = std::env::current_dir().unwrap();
        let mut runtime = JsRuntime::new(render_state, input_state, root).unwrap();
        runtime
            .execute_source(
                r#"
                globalThis.__fetchProbe = false;
                globalThis.__dataFetchProbe = '';
                globalThis.__imageProbe = false;
                fetch("Cargo.toml")
                  .then(async (response) => {
                    const buffer = await response.arrayBuffer();
                    const bytes = new Uint8Array(buffer);
                    return response.ok && response.status === 200 && bytes[0] === 91;
                  })
                  .then((result) => { globalThis.__fetchProbe = result; });
                fetch(new Request("data:text/plain;base64,SGk="))
                  .then((response) => response.text())
                  .then((result) => { globalThis.__dataFetchProbe = result; });
                fetch("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
                  .then((response) => response.blob())
                  .then((blob) => createImageBitmap(blob))
                  .then((bitmap) => {
                    globalThis.__imageProbe = bitmap.width === 1 && bitmap.height === 1 && bitmap.data.byteLength === 4;
                  });
                "#,
            )
            .unwrap();
        runtime
            .execute_source(
                "if (globalThis.__fetchProbe !== true || globalThis.__dataFetchProbe !== 'Hi' || globalThis.__imageProbe !== true) throw new Error('fetch/image probe failed');",
            )
            .unwrap();
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
