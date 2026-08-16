use crate::bridge::SharedRenderState;
use anyhow::{Context as _, Result};
use boa_engine::{
    js_string, Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction, Source,
};
use std::path::Path;

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
    pub fn new(render_state: SharedRenderState) -> Result<Self> {
        let mut context = Context::default();
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

        let camera_state = render_state;
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

        Ok(Self { context })
    }

    pub fn execute_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read JavaScript entry point {}", path.display()))?;
        self.execute_source(&source)
    }

    pub fn execute_source(&mut self, source: &str) -> Result<()> {
        self.context
            .eval(Source::from_bytes(source))
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("JavaScript evaluation failed: {error}"))
    }
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
