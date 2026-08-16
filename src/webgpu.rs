use anyhow::Result;
use boa_engine::{js_string, Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub struct NativeWebGpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    resources: Mutex<Resources>,
}

#[derive(Debug, Default)]
struct Resources {
    next_id: u64,
    buffers: HashMap<u64, wgpu::Buffer>,
    textures: HashMap<u64, wgpu::Texture>,
    shader_modules: HashMap<u64, wgpu::ShaderModule>,
}

pub type SharedNativeWebGpuContext = Arc<NativeWebGpuContext>;

impl NativeWebGpuContext {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> SharedNativeWebGpuContext {
        Arc::new(Self {
            device,
            queue,
            resources: Mutex::new(Resources {
                next_id: 1,
                ..Resources::default()
            }),
        })
    }

    fn allocate_id(&self) -> Result<u64, String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let id = resources.next_id;
        resources.next_id = resources
            .next_id
            .checked_add(1)
            .ok_or_else(|| "WebGPU resource id space exhausted".to_string())?;
        Ok(id)
    }

    fn create_buffer(&self, size: u64, usage: u64) -> Result<u64, String> {
        if size == 0 {
            return Err("GPUBuffer size must be positive".to_string());
        }
        let id = self.allocate_id()?;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hyperthree-js-gpu-buffer"),
            size,
            usage: buffer_usage(usage),
            mapped_at_creation: false,
        });
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .buffers
            .insert(id, buffer);
        Ok(id)
    }

    fn write_buffer(&self, id: u64, offset: u64, bytes: &[u8]) -> Result<(), String> {
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let buffer = resources
            .buffers
            .get(&id)
            .ok_or_else(|| format!("unknown GPUBuffer handle {id}"))?;
        self.queue.write_buffer(buffer, offset, bytes);
        Ok(())
    }

    fn destroy_buffer(&self, id: u64) -> Result<(), String> {
        let buffer = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .buffers
            .remove(&id)
            .ok_or_else(|| format!("unknown GPUBuffer handle {id}"))?;
        buffer.destroy();
        Ok(())
    }

    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: &str,
        usage: u64,
    ) -> Result<u64, String> {
        if width == 0 || height == 0 {
            return Err("GPUTexture dimensions must be positive".to_string());
        }
        let id = self.allocate_id()?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hyperthree-js-gpu-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format(format),
            usage: texture_usage(usage),
            view_formats: &[],
        });
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .textures
            .insert(id, texture);
        Ok(id)
    }

    fn write_texture(&self, id: u64, width: u32, height: u32, bytes: &[u8]) -> Result<(), String> {
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let texture = resources
            .textures
            .get(&id)
            .ok_or_else(|| format!("unknown GPUTexture handle {id}"))?;
        let expected = width as usize * height as usize * 4;
        if bytes.len() < expected {
            return Err(format!(
                "GPUTexture upload is {} bytes, expected at least {expected}",
                bytes.len()
            ));
        }
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    fn create_shader_module(&self, source: &str) -> Result<u64, String> {
        let id = self.allocate_id()?;
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hyperthree-js-shader-module"),
                source: wgpu::ShaderSource::Wgsl(source.to_string().into()),
            });
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .shader_modules
            .insert(id, shader);
        Ok(id)
    }
}

pub fn register_bindings(
    context: &mut Context,
    gpu: Option<SharedNativeWebGpuContext>,
) -> Result<()> {
    let Some(gpu) = gpu else {
        return Ok(());
    };

    let create_buffer_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuCreateBuffer",
        2,
        move |_this, args, context| {
            let size = number_arg(args, 0, context)? as u64;
            let usage = number_arg(args, 1, context)? as u64;
            create_buffer_gpu
                .create_buffer(size, usage)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let write_buffer_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuWriteBuffer",
        3,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            let offset = number_arg(args, 1, context)? as u64;
            let bytes = byte_array_arg(args, 2, context)?;
            write_buffer_gpu
                .write_buffer(id, offset, &bytes)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let destroy_buffer_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDestroyBuffer",
        1,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            destroy_buffer_gpu
                .destroy_buffer(id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let create_texture_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuCreateTexture",
        4,
        move |_this, args, context| {
            let width = number_arg(args, 0, context)? as u32;
            let height = number_arg(args, 1, context)? as u32;
            let format = string_arg(args, 2, context)?;
            let usage = number_arg(args, 3, context)? as u64;
            create_texture_gpu
                .create_texture(width, height, &format, usage)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let write_texture_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuWriteTexture",
        4,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            let width = number_arg(args, 1, context)? as u32;
            let height = number_arg(args, 2, context)? as u32;
            let bytes = byte_array_arg(args, 3, context)?;
            write_texture_gpu
                .write_texture(id, width, height, &bytes)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let shader_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuCreateShaderModule",
        1,
        move |_this, args, context| {
            let source = string_arg(args, 0, context)?;
            shader_gpu
                .create_shader_module(&source)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    register(
        context,
        "__hyperthreeWebGpuSubmit",
        0,
        |_this, _args, _context| Ok(JsValue::undefined()),
    )?;

    context
        .eval(boa_engine::Source::from_bytes(WEBGPU_BOOTSTRAP))
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("failed to install native WebGPU objects: {error}"))
}

fn register<F>(context: &mut Context, name: &str, length: usize, callback: F) -> Result<()>
where
    F: Fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue> + 'static,
{
    context
        .register_global_builtin_callable(js_string!(name), length, unsafe {
            NativeFunction::from_closure(callback)
        })
        .map_err(|error| anyhow::anyhow!("failed to register WebGPU binding {name}: {error}"))?;
    Ok(())
}

fn number_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<f64> {
    let value = args.get_or_undefined(index).to_number(context)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(JsNativeError::range()
            .with_message("WebGPU numeric argument must be finite")
            .into())
    }
}

fn string_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    args.get_or_undefined(index)
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .map_err(|_| {
            JsNativeError::typ()
                .with_message("WebGPU argument must be a string")
                .into()
        })
}

fn byte_array_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<Vec<u8>> {
    let object = args
        .get_or_undefined(index)
        .to_object(context)
        .map_err(|_| JsNativeError::typ().with_message("WebGPU data must be array-like"))?;
    let length = object
        .get(js_string!("length"), context)?
        .to_length(context)
        .map_err(|_| JsNativeError::typ().with_message("WebGPU data length is invalid"))?;
    let mut bytes = Vec::with_capacity(length as usize);
    for index in 0..length as usize {
        let value = object.get(index, context)?.to_number(context)?;
        if !value.is_finite() || !(0.0..=255.0).contains(&value) || value.fract() != 0.0 {
            return Err(JsNativeError::range()
                .with_message("WebGPU byte data is invalid")
                .into());
        }
        bytes.push(value as u8);
    }
    Ok(bytes)
}

fn native_error(error: String) -> boa_engine::JsError {
    JsNativeError::error().with_message(error).into()
}

fn buffer_usage(bits: u64) -> wgpu::BufferUsages {
    let mut usage = wgpu::BufferUsages::empty();
    if bits & 1 != 0 {
        usage |= wgpu::BufferUsages::MAP_READ;
    }
    if bits & 2 != 0 {
        usage |= wgpu::BufferUsages::MAP_WRITE;
    }
    if bits & 4 != 0 {
        usage |= wgpu::BufferUsages::COPY_SRC;
    }
    if bits & 8 != 0 {
        usage |= wgpu::BufferUsages::COPY_DST;
    }
    if bits & 16 != 0 {
        usage |= wgpu::BufferUsages::INDEX;
    }
    if bits & 32 != 0 {
        usage |= wgpu::BufferUsages::VERTEX;
    }
    if bits & 64 != 0 {
        usage |= wgpu::BufferUsages::UNIFORM;
    }
    if bits & 128 != 0 {
        usage |= wgpu::BufferUsages::STORAGE;
    }
    usage
}

fn texture_usage(bits: u64) -> wgpu::TextureUsages {
    let mut usage = wgpu::TextureUsages::empty();
    if bits & 1 != 0 {
        usage |= wgpu::TextureUsages::COPY_SRC;
    }
    if bits & 2 != 0 {
        usage |= wgpu::TextureUsages::COPY_DST;
    }
    if bits & 4 != 0 {
        usage |= wgpu::TextureUsages::TEXTURE_BINDING;
    }
    if bits & 8 != 0 {
        usage |= wgpu::TextureUsages::STORAGE_BINDING;
    }
    if bits & 16 != 0 {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    usage
}

fn texture_format(format: &str) -> wgpu::TextureFormat {
    match format {
        "bgra8unorm" => wgpu::TextureFormat::Bgra8Unorm,
        "bgra8unorm-srgb" => wgpu::TextureFormat::Bgra8UnormSrgb,
        "rgba16float" => wgpu::TextureFormat::Rgba16Float,
        _ => wgpu::TextureFormat::Rgba8Unorm,
    }
}

const WEBGPU_BOOTSTRAP: &str = r#"
(() => {
  if (globalThis.navigator?.gpu) return;
  const makeHandle = (id, methods = {}) => Object.assign({ __hyperthreeHandle: id }, methods);
  const bufferUsage = { MAP_READ: 1, MAP_WRITE: 2, COPY_SRC: 4, COPY_DST: 8, INDEX: 16, VERTEX: 32, UNIFORM: 64, STORAGE: 128 };
  const textureUsage = { COPY_SRC: 1, COPY_DST: 2, TEXTURE_BINDING: 4, STORAGE_BINDING: 8, RENDER_ATTACHMENT: 16 };
  globalThis.GPUBufferUsage = globalThis.GPUBufferUsage || bufferUsage;
  globalThis.GPUTextureUsage = globalThis.GPUTextureUsage || textureUsage;
  const makeBuffer = (descriptor = {}) => {
    const id = __hyperthreeWebGpuCreateBuffer(descriptor.size ?? 1, descriptor.usage ?? 0);
    return makeHandle(id, { destroy: () => __hyperthreeWebGpuDestroyBuffer(id) });
  };
  const makeTexture = (descriptor = {}) => {
    const size = descriptor.size || {};
    const width = typeof size === 'number' ? size : (size.width ?? 1);
    const height = typeof size === 'number' ? 1 : (size.height ?? 1);
    const id = __hyperthreeWebGpuCreateTexture(width, height, descriptor.format ?? 'rgba8unorm', descriptor.usage ?? 4);
    return makeHandle(id, { createView: (viewDescriptor = {}) => makeHandle(id, { __textureView: true, descriptor: viewDescriptor }) });
  };
  const makeDevice = () => {
    const queue = {
      writeBuffer(buffer, offset, data) {
        const bytes = new Uint8Array(data.buffer, data.byteOffset ?? 0, data.byteLength ?? data.length);
        __hyperthreeWebGpuWriteBuffer(buffer.__hyperthreeHandle, offset, bytes);
      },
      writeTexture(destination, data, dataLayout, size) {
        const bytes = new Uint8Array(data.buffer, data.byteOffset ?? 0, data.byteLength ?? data.length);
        __hyperthreeWebGpuWriteTexture(destination.texture.__hyperthreeHandle, size.width, size.height, bytes);
      },
      submit(commandBuffers) { __hyperthreeWebGpuSubmit(commandBuffers?.length ?? 0); },
    };
    const device = {
      queue,
      features: new Set(),
      limits: {},
      createBuffer: makeBuffer,
      createTexture: makeTexture,
      createShaderModule(descriptor = {}) {
        return makeHandle(__hyperthreeWebGpuCreateShaderModule(descriptor.code ?? ''));
      },
      createBindGroupLayout: (descriptor = {}) => makeHandle({ descriptor }),
      createPipelineLayout: (descriptor = {}) => makeHandle({ descriptor }),
      createBindGroup: (descriptor = {}) => makeHandle({ descriptor }),
      createSampler: (descriptor = {}) => makeHandle({ descriptor }),
      createRenderPipeline: (descriptor = {}) => makeHandle({ descriptor, getBindGroupLayout: () => makeHandle({}) }),
      createRenderPipelineAsync: async (descriptor = {}) => device.createRenderPipeline(descriptor),
      createComputePipeline: (descriptor = {}) => makeHandle({ descriptor }),
      createComputePipelineAsync: async (descriptor = {}) => device.createComputePipeline(descriptor),
      createCommandEncoder: () => ({
        beginRenderPass: () => makePass(),
        beginComputePass: () => makePass(),
        copyBufferToBuffer() {},
        copyTextureToTexture() {},
        finish: () => ({}),
      }),
      pushErrorScope() {}, popErrorScope: async () => null,
      lost: Promise.resolve({ reason: '', message: '' }),
    };
    return device;
  };
  const makePass = () => ({
    setPipeline() {}, setBindGroup() {}, setVertexBuffer() {}, setIndexBuffer() {},
    setViewport() {}, setScissorRect() {}, setBlendConstant() {}, setStencilReference() {},
    draw() {}, drawIndexed() {}, drawIndirect() {}, drawIndexedIndirect() {},
    dispatchWorkgroups() {}, dispatchWorkgroupsIndirect() {}, end() {}, endPass() {},
  });
  const adapter = {
    name: 'HyperThree Native wgpu',
    features: new Set(),
    limits: {},
    isFallbackAdapter: false,
    requestDevice: async () => makeDevice(),
  };
  globalThis.navigator = globalThis.navigator || {};
  globalThis.navigator.gpu = {
    requestAdapter: async () => adapter,
    getPreferredCanvasFormat: () => 'bgra8unorm-srgb',
  };
})();
"#;
