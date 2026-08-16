use anyhow::Result;
use boa_engine::{
    js_string,
    object::builtins::{AlignedVec, JsArrayBuffer},
    Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Debug)]
pub struct NativeWebGpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Arc<wgpu::Surface<'static>>,
    surface_config: Mutex<wgpu::SurfaceConfiguration>,
    supported_surface_formats: Vec<wgpu::TextureFormat>,
    supported_alpha_modes: Vec<wgpu::CompositeAlphaMode>,
    features: wgpu::Features,
    configured: AtomicBool,
    presented_this_frame: AtomicBool,
    device_events: Arc<DeviceEvents>,
    resources: Mutex<Resources>,
}

#[derive(Debug, Default)]
struct DeviceEvents {
    lost: Mutex<Option<DeviceLostEvent>>,
}

#[derive(Debug, Clone)]
struct DeviceLostEvent {
    reason: String,
    message: String,
}

#[derive(Debug, Default)]
struct Resources {
    next_id: u64,
    buffers: HashMap<u64, wgpu::Buffer>,
    textures: HashMap<u64, wgpu::Texture>,
    texture_views: HashMap<u64, wgpu::TextureView>,
    samplers: HashMap<u64, wgpu::Sampler>,
    shader_modules: HashMap<u64, wgpu::ShaderModule>,
    bind_group_layouts: HashMap<u64, wgpu::BindGroupLayout>,
    pipeline_layouts: HashMap<u64, wgpu::PipelineLayout>,
    bind_groups: HashMap<u64, wgpu::BindGroup>,
    render_pipelines: HashMap<u64, wgpu::RenderPipeline>,
    compute_pipelines: HashMap<u64, wgpu::ComputePipeline>,
    query_sets: HashMap<u64, wgpu::QuerySet>,
    render_bundle_encoders: HashMap<u64, RenderBundleEncoderState>,
    render_bundles: HashMap<u64, wgpu::RenderBundle>,
    command_encoders: HashMap<u64, CommandEncoderState>,
    open_passes: HashMap<u64, OpenPass>,
    command_buffers: HashMap<u64, QueuedCommandBuffer>,
    surface_textures: HashMap<u64, wgpu::SurfaceTexture>,
    texture_view_sources: HashMap<u64, u64>,
}

struct TextureCreateInfo<'a> {
    width: u32,
    height: u32,
    depth: u32,
    format: &'a str,
    usage: u64,
    mip_level_count: u32,
    sample_count: u32,
    dimension: &'a str,
}

struct TextureWriteInfo {
    id: u64,
    mip_level: u32,
    width: u32,
    height: u32,
    depth: u32,
    bytes_per_row: u32,
    rows_per_image: u32,
    origin: [u32; 3],
}

#[derive(Debug)]
struct QueuedCommandBuffer {
    buffer: wgpu::CommandBuffer,
    surface_textures: Vec<u64>,
}

#[derive(Debug, Default)]
struct CommandEncoderState {
    commands: Vec<RecordedCommand>,
}

#[derive(Debug)]
enum RecordedCommand {
    RenderPass(RenderPassState),
    ComputePass(ComputePassState),
    ClearBuffer {
        buffer: u64,
        offset: u64,
        size: Option<u64>,
    },
    CopyBufferToBuffer {
        source: u64,
        source_offset: u64,
        destination: u64,
        destination_offset: u64,
        size: u64,
    },
    CopyBufferToTexture {
        source: u64,
        source_offset: u64,
        bytes_per_row: u32,
        rows_per_image: u32,
        destination: u64,
        destination_mip_level: u32,
        destination_origin: [u32; 3],
        size: [u32; 3],
    },
    CopyTextureToTexture {
        source: u64,
        source_mip_level: u32,
        source_origin: [u32; 3],
        destination: u64,
        destination_mip_level: u32,
        destination_origin: [u32; 3],
        size: [u32; 3],
    },
    CopyTextureToBuffer {
        source: u64,
        source_mip_level: u32,
        source_origin: [u32; 3],
        destination: u64,
        destination_offset: u64,
        bytes_per_row: u32,
        rows_per_image: u32,
        size: [u32; 3],
    },
    ResolveQuerySet {
        query_set: u64,
        first_query: u32,
        query_count: u32,
        destination: u64,
        destination_offset: u64,
    },
    WriteTimestamp {
        query_set: u64,
        query_index: u32,
    },
}

#[derive(Debug)]
enum OpenPass {
    Render {
        encoder: u64,
        state: RenderPassState,
    },
    Compute {
        encoder: u64,
        state: ComputePassState,
    },
}

#[derive(Debug)]
struct RenderPassState {
    colors: Vec<ColorAttachmentState>,
    depth_stencil: Option<DepthStencilAttachmentState>,
    occlusion_query_set: Option<u64>,
    timestamp_writes: Option<TimestampWrites>,
    commands: Vec<RenderCommand>,
}

#[derive(Debug)]
struct ComputePassState {
    timestamp_writes: Option<TimestampWrites>,
    commands: Vec<ComputeCommand>,
}

#[derive(Debug, Clone, Copy)]
struct TimestampWrites {
    query_set: u64,
    beginning_of_pass_write_index: Option<u32>,
    end_of_pass_write_index: Option<u32>,
}

#[derive(Debug)]
struct RenderBundleEncoderState {
    color_formats: Vec<Option<wgpu::TextureFormat>>,
    depth_stencil_format: Option<wgpu::RenderBundleDepthStencil>,
    sample_count: u32,
    commands: Vec<RenderCommand>,
}

#[derive(Debug)]
struct ColorAttachmentState {
    view: u64,
    resolve_target: Option<u64>,
    load_op: String,
    store_op: String,
    clear_value: [f64; 4],
}

#[derive(Debug)]
struct DepthStencilAttachmentState {
    view: u64,
    depth_load_op: String,
    depth_store_op: String,
    depth_clear_value: f32,
}

#[derive(Debug)]
enum RenderCommand {
    SetPipeline(u64),
    SetBindGroup {
        index: u32,
        bind_group: u64,
        dynamic_offsets: Vec<u32>,
    },
    SetVertexBuffer {
        slot: u32,
        buffer: u64,
        offset: u64,
        size: Option<u64>,
    },
    SetIndexBuffer {
        buffer: u64,
        offset: u64,
        format: String,
    },
    SetViewport {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    },
    SetScissorRect {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    SetBlendConstant([f64; 4]),
    SetStencilReference(u32),
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    },
    DrawIndirect {
        buffer: u64,
        offset: u64,
    },
    DrawIndexedIndirect {
        buffer: u64,
        offset: u64,
    },
    BeginOcclusionQuery(u32),
    EndOcclusionQuery,
    ExecuteBundles(Vec<u64>),
}

#[derive(Debug)]
enum ComputeCommand {
    SetPipeline(u64),
    SetBindGroup {
        index: u32,
        bind_group: u64,
        dynamic_offsets: Vec<u32>,
    },
    DispatchWorkgroups {
        x: u32,
        y: u32,
        z: u32,
    },
    DispatchWorkgroupsIndirect {
        buffer: u64,
        offset: u64,
    },
}

pub type SharedNativeWebGpuContext = Arc<NativeWebGpuContext>;

impl NativeWebGpuContext {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface: Arc<wgpu::Surface<'static>>,
        surface_config: wgpu::SurfaceConfiguration,
        features: wgpu::Features,
        supported_surface_formats: Vec<wgpu::TextureFormat>,
        supported_alpha_modes: Vec<wgpu::CompositeAlphaMode>,
    ) -> SharedNativeWebGpuContext {
        let device_events = Arc::new(DeviceEvents::default());
        let lost_events = Arc::clone(&device_events);
        device.set_device_lost_callback(move |reason, message| {
            let reason = match reason {
                wgpu::DeviceLostReason::Destroyed => "destroyed",
                wgpu::DeviceLostReason::Dropped => "dropped",
                wgpu::DeviceLostReason::DeviceInvalid => "device-invalid",
                wgpu::DeviceLostReason::ReplacedCallback => "replaced-callback",
                wgpu::DeviceLostReason::Unknown => "unknown",
            };
            if let Ok(mut lost) = lost_events.lost.lock() {
                if lost.is_none() {
                    *lost = Some(DeviceLostEvent {
                        reason: reason.to_string(),
                        message,
                    });
                }
            }
        });
        Arc::new(Self {
            device,
            queue,
            surface,
            surface_config: Mutex::new(surface_config),
            supported_surface_formats,
            supported_alpha_modes,
            features,
            configured: AtomicBool::new(false),
            presented_this_frame: AtomicBool::new(false),
            device_events,
            resources: Mutex::new(Resources {
                next_id: 1,
                ..Resources::default()
            }),
        })
    }

    pub fn take_presented_this_frame(&self) -> bool {
        self.presented_this_frame.swap(false, Ordering::AcqRel)
    }

    fn webgpu_feature_names(&self) -> Vec<&'static str> {
        let mut features = Vec::new();
        if self.features.contains(wgpu::Features::DEPTH_CLIP_CONTROL) {
            features.push("depth-clip-control");
        }
        if self
            .features
            .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
        {
            features.push("depth32float-stencil8");
        }
        if self
            .features
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        {
            features.push("texture-compression-bc");
        }
        if self
            .features
            .contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2)
        {
            features.push("texture-compression-etc2");
        }
        if self
            .features
            .contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC)
        {
            features.push("texture-compression-astc");
        }
        if self.features.contains(wgpu::Features::TIMESTAMP_QUERY)
            && self
                .features
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
        {
            features.push("timestamp-query");
        }
        if self
            .features
            .contains(wgpu::Features::INDIRECT_FIRST_INSTANCE)
        {
            features.push("indirect-first-instance");
        }
        if self
            .features
            .contains(wgpu::Features::RG11B10UFLOAT_RENDERABLE)
        {
            features.push("rg11b10ufloat-renderable");
        }
        if self.features.contains(wgpu::Features::BGRA8UNORM_STORAGE) {
            features.push("bgra8unorm-storage");
        }
        if self.features.contains(wgpu::Features::FLOAT32_FILTERABLE) {
            features.push("float32-filterable");
        }
        features
    }

    fn webgpu_limits(&self) -> Value {
        let limits = self.device.limits();
        serde_json::json!({
            "maxTextureDimension1D": limits.max_texture_dimension_1d,
            "maxTextureDimension2D": limits.max_texture_dimension_2d,
            "maxTextureDimension3D": limits.max_texture_dimension_3d,
            "maxTextureArrayLayers": limits.max_texture_array_layers,
            "maxBindGroups": limits.max_bind_groups,
            "maxBindingsPerBindGroup": limits.max_bindings_per_bind_group,
            "maxDynamicUniformBuffersPerPipelineLayout": limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            "maxDynamicStorageBuffersPerPipelineLayout": limits.max_dynamic_storage_buffers_per_pipeline_layout,
            "maxSampledTexturesPerShaderStage": limits.max_sampled_textures_per_shader_stage,
            "maxSamplersPerShaderStage": limits.max_samplers_per_shader_stage,
            "maxStorageBuffersPerShaderStage": limits.max_storage_buffers_per_shader_stage,
            "maxStorageTexturesPerShaderStage": limits.max_storage_textures_per_shader_stage,
            "maxUniformBuffersPerShaderStage": limits.max_uniform_buffers_per_shader_stage,
            "maxUniformBufferBindingSize": limits.max_uniform_buffer_binding_size,
            "maxStorageBufferBindingSize": limits.max_storage_buffer_binding_size,
            "maxVertexBuffers": limits.max_vertex_buffers,
            "maxBufferSize": limits.max_buffer_size,
            "maxVertexAttributes": limits.max_vertex_attributes,
            "maxVertexBufferArrayStride": limits.max_vertex_buffer_array_stride,
            "minUniformBufferOffsetAlignment": limits.min_uniform_buffer_offset_alignment,
            "minStorageBufferOffsetAlignment": limits.min_storage_buffer_offset_alignment,
            "maxInterStageShaderComponents": limits.max_inter_stage_shader_components,
            "maxColorAttachments": limits.max_color_attachments,
            "maxColorAttachmentBytesPerSample": limits.max_color_attachment_bytes_per_sample,
            "maxComputeWorkgroupStorageSize": limits.max_compute_workgroup_storage_size,
            "maxComputeInvocationsPerWorkgroup": limits.max_compute_invocations_per_workgroup,
            "maxComputeWorkgroupSizeX": limits.max_compute_workgroup_size_x,
            "maxComputeWorkgroupSizeY": limits.max_compute_workgroup_size_y,
            "maxComputeWorkgroupSizeZ": limits.max_compute_workgroup_size_z,
            "maxComputeWorkgroupsPerDimension": limits.max_compute_workgroups_per_dimension,
            "minSubgroupSize": limits.min_subgroup_size,
            "maxSubgroupSize": limits.max_subgroup_size,
            "maxPushConstantSize": limits.max_push_constant_size,
            "maxNonSamplerBindings": limits.max_non_sampler_bindings,
        })
    }

    fn wait_for_submitted_work(&self) {
        self.device.poll(wgpu::Maintain::Wait);
    }

    /// Returns the native loss record after wgpu has reported a device loss.
    /// The JS `GPUDevice.lost` promise uses the same record; the host uses it
    /// to stop before submitting more work to an invalid device.
    pub fn device_lost_message(&self) -> Option<String> {
        self.device.poll(wgpu::Maintain::Poll);
        self.device_events
            .lost
            .lock()
            .ok()?
            .as_ref()
            .map(|event| format!("{}: {}", event.reason, event.message))
    }

    pub fn resize_surface(&self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        let mut config = self
            .surface_config
            .lock()
            .map_err(|_| "WebGPU surface configuration poisoned".to_string())?;
        config.width = width;
        config.height = height;
        self.surface.configure(&self.device, &config);
        Ok(())
    }

    fn canvas_configuration(&self) -> Result<Value, String> {
        let config = self
            .surface_config
            .lock()
            .map_err(|_| "WebGPU surface configuration poisoned".to_string())?;
        let alpha_mode = if config.alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied {
            "premultiplied"
        } else {
            "opaque"
        };
        Ok(serde_json::json!({
            "alphaMode": alpha_mode,
            "width": config.width,
            "height": config.height,
        }))
    }

    fn configure_surface(&self, descriptor: &Value) -> Result<(), String> {
        let mut config = self
            .surface_config
            .lock()
            .map_err(|_| "WebGPU surface configuration poisoned".to_string())?;
        if let Some(format_name) = descriptor.get("format").and_then(Value::as_str) {
            let format = texture_format(format_name);
            if !self.supported_surface_formats.contains(&format) {
                return Err(format!(
                    "WebGPU canvas format {format_name} is not supported by the native surface"
                ));
            }
            config.format = format;
        }
        if let Some(alpha_name) = descriptor.get("alphaMode").and_then(Value::as_str) {
            let alpha_mode = match alpha_name {
                "opaque" => wgpu::CompositeAlphaMode::Opaque,
                "premultiplied" => wgpu::CompositeAlphaMode::PreMultiplied,
                other => return Err(format!("unsupported WebGPU canvas alpha mode {other}")),
            };
            let alpha_mode = if self.supported_alpha_modes.contains(&alpha_mode) {
                alpha_mode
            } else if alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied
                && self
                    .supported_alpha_modes
                    .contains(&wgpu::CompositeAlphaMode::Opaque)
            {
                // Some native surfaces expose opaque composition only. Keep the
                // standard Three.js initialization path usable and preserve the
                // surface's valid presentation mode; callers requiring transparent
                // presentation must use a surface that advertises premultiplied
                // alpha.
                wgpu::CompositeAlphaMode::Opaque
            } else {
                return Err(format!(
                    "WebGPU canvas alpha mode {alpha_name} is not supported by the native surface"
                ));
            };
            config.alpha_mode = alpha_mode;
        }
        self.surface.configure(&self.device, &config);
        self.configured.store(true, Ordering::Release);
        Ok(())
    }

    fn unconfigure_surface(&self) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let surface_texture_ids = resources
            .surface_textures
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for texture_id in surface_texture_ids {
            remove_surface_texture_views(&mut resources, texture_id);
            resources.surface_textures.remove(&texture_id);
        }
        self.configured.store(false, Ordering::Release);
        Ok(())
    }

    fn poll_device_lost(&self) -> Option<String> {
        self.device.poll(wgpu::Maintain::Poll);
        let lost = self.device_events.lost.lock().ok()?.clone()?;
        serde_json::to_string(&serde_json::json!({
            "reason": lost.reason,
            "message": lost.message,
        }))
        .ok()
    }

    fn destroy_device(&self) {
        self.device.destroy();
    }

    fn push_error_scope(&self, filter: &str) -> Result<(), String> {
        let filter = match filter {
            "out-of-memory" => wgpu::ErrorFilter::OutOfMemory,
            "validation" => wgpu::ErrorFilter::Validation,
            "internal" => wgpu::ErrorFilter::Internal,
            other => return Err(format!("unsupported GPU error scope filter {other}")),
        };
        self.device.push_error_scope(filter);
        Ok(())
    }

    fn pop_error_scope(&self) -> Option<String> {
        pollster::block_on(self.device.pop_error_scope()).map(|error| error.to_string())
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

    fn read_buffer(&self, id: u64, offset: u64, size: u64) -> Result<Vec<u8>, String> {
        if size == 0 {
            return Ok(Vec::new());
        }
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let buffer = resources
            .buffers
            .get(&id)
            .ok_or_else(|| format!("unknown GPUBuffer handle {id}"))?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| "GPUBuffer read range overflowed".to_string())?;
        if end > buffer.size() {
            return Err(format!(
                "GPUBuffer read range {offset}..{end} exceeds {}",
                buffer.size()
            ));
        }
        let slice = buffer.slice(offset..end);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result.map_err(|error| error.to_string()));
        });
        self.device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .map_err(|_| "GPUBuffer map callback was dropped".to_string())??;
        let bytes = slice.get_mapped_range().to_vec();
        buffer.unmap();
        Ok(bytes)
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

    fn create_texture(&self, info: TextureCreateInfo<'_>) -> Result<u64, String> {
        if info.width == 0 || info.height == 0 || info.depth == 0 {
            return Err("GPUTexture dimensions must be positive".to_string());
        }
        if info.mip_level_count == 0 || info.sample_count == 0 {
            return Err("GPUTexture mip and sample counts must be positive".to_string());
        }
        let id = self.allocate_id()?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hyperthree-js-gpu-texture"),
            size: wgpu::Extent3d {
                width: info.width,
                height: info.height,
                depth_or_array_layers: info.depth,
            },
            mip_level_count: info.mip_level_count,
            sample_count: info.sample_count,
            dimension: texture_dimension(info.dimension),
            format: texture_format(info.format),
            usage: texture_usage_for_dimension(info.usage, info.dimension),
            view_formats: &[],
        });
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .textures
            .insert(id, texture);
        Ok(id)
    }

    fn destroy_texture(&self, id: u64) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let texture = resources
            .textures
            .remove(&id)
            .ok_or_else(|| format!("unknown GPUTexture handle {id}"))?;
        let stale_views = resources
            .texture_view_sources
            .iter()
            .filter_map(|(view_id, source_id)| (*source_id == id).then_some(*view_id))
            .collect::<Vec<_>>();
        for view_id in stale_views {
            resources.texture_view_sources.remove(&view_id);
            resources.texture_views.remove(&view_id);
        }
        texture.destroy();
        Ok(())
    }

    fn write_texture(&self, info: TextureWriteInfo, bytes: &[u8]) -> Result<(), String> {
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let texture = resources
            .textures
            .get(&info.id)
            .ok_or_else(|| format!("unknown GPUTexture handle {}", info.id))?;
        let expected =
            info.bytes_per_row as usize * info.rows_per_image as usize * info.depth as usize;
        if bytes.len() < expected {
            return Err(format!(
                "GPUTexture upload is {} bytes, expected at least {expected}",
                bytes.len()
            ));
        }
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: info.mip_level,
                origin: wgpu::Origin3d {
                    x: info.origin[0],
                    y: info.origin[1],
                    z: info.origin[2],
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(info.bytes_per_row),
                rows_per_image: Some(info.rows_per_image),
            },
            wgpu::Extent3d {
                width: info.width,
                height: info.height,
                depth_or_array_layers: info.depth,
            },
        );
        Ok(())
    }

    fn create_shader_module(&self, source: &str) -> Result<u64, String> {
        let id = self.allocate_id()?;
        let source = sanitize_wgsl(source);
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hyperthree-js-shader-module"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .shader_modules
            .insert(id, shader);
        Ok(id)
    }

    fn create_texture_view(&self, texture_id: u64, descriptor: &Value) -> Result<u64, String> {
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let view_descriptor = texture_view_descriptor(descriptor);
        let view = if let Some(texture) = resources.textures.get(&texture_id) {
            texture.create_view(&view_descriptor)
        } else if let Some(surface_texture) = resources.surface_textures.get(&texture_id) {
            surface_texture.texture.create_view(&view_descriptor)
        } else {
            return Err(format!("unknown GPUTexture handle {texture_id}"));
        };
        drop(resources);
        let id = self.allocate_id()?;
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        resources.texture_views.insert(id, view);
        resources.texture_view_sources.insert(id, texture_id);
        Ok(id)
    }

    fn get_current_surface_texture(&self) -> Result<u64, String> {
        if !self.configured.load(Ordering::Acquire) {
            return Err("WebGPU canvas context is not configured".to_string());
        }
        let surface_texture = match self.surface.get_current_texture() {
            Ok(surface_texture) => surface_texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let config = self
                    .surface_config
                    .lock()
                    .map_err(|_| "WebGPU surface configuration poisoned".to_string())?
                    .clone();
                self.surface.configure(&self.device, &config);
                self.surface.get_current_texture().map_err(|error| {
                    format!("failed to reacquire WebGPU canvas texture after reconfigure: {error}")
                })?
            }
            Err(error) => return Err(format!("failed to acquire WebGPU canvas texture: {error}")),
        };
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .surface_textures
            .insert(id, surface_texture);
        Ok(id)
    }

    fn discard_surface_texture(&self, id: u64) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        resources.surface_textures.remove(&id);
        remove_surface_texture_views(&mut resources, id);
        Ok(())
    }

    fn create_sampler(&self, descriptor: &Value) -> Result<u64, String> {
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hyperthree-js-sampler"),
            address_mode_u: address_mode(json_string(descriptor, "addressModeU")),
            address_mode_v: address_mode(json_string(descriptor, "addressModeV")),
            address_mode_w: address_mode(json_string(descriptor, "addressModeW")),
            mag_filter: filter_mode(json_string(descriptor, "magFilter")),
            min_filter: filter_mode(json_string(descriptor, "minFilter")),
            mipmap_filter: filter_mode(json_string(descriptor, "mipmapFilter")),
            lod_min_clamp: json_f32(descriptor, "lodMinClamp", 0.0),
            lod_max_clamp: json_f32(descriptor, "lodMaxClamp", 32.0),
            compare: json_string(descriptor, "compare")
                .map(|compare| compare_function(Some(compare))),
            anisotropy_clamp: json_u16(descriptor, "maxAnisotropy", 1),
            border_color: None,
        });
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .samplers
            .insert(id, sampler);
        Ok(id)
    }

    fn destroy_sampler(&self, id: u64) -> Result<(), String> {
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .samplers
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| format!("unknown GPUSampler handle {id}"))
    }

    fn create_bind_group_layout(&self, descriptor: &Value) -> Result<u64, String> {
        let entries = descriptor
            .get("entries")
            .and_then(Value::as_array)
            .ok_or_else(|| "GPUBindGroupLayout.entries must be an array".to_string())?;
        let entries = entries
            .iter()
            .map(bind_group_layout_entry)
            .collect::<Result<Vec<_>, _>>()?;
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hyperthree-js-bind-group-layout"),
                entries: &entries,
            });
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .bind_group_layouts
            .insert(id, layout);
        Ok(id)
    }

    fn create_pipeline_layout(&self, descriptor: &Value) -> Result<u64, String> {
        let ids = descriptor
            .get("bindGroupLayouts")
            .and_then(Value::as_array)
            .ok_or_else(|| "GPUPipelineLayout.bindGroupLayouts must be an array".to_string())?
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| "invalid bind group layout handle".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let layouts = ids
            .iter()
            .map(|id| {
                resources
                    .bind_group_layouts
                    .get(id)
                    .ok_or_else(|| format!("unknown GPUBindGroupLayout handle {id}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hyperthree-js-pipeline-layout"),
                bind_group_layouts: &layouts,
                push_constant_ranges: &[],
            });
        drop(resources);
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .pipeline_layouts
            .insert(id, layout);
        Ok(id)
    }

    fn create_bind_group(&self, descriptor: &Value) -> Result<u64, String> {
        let layout_id = descriptor
            .get("layout")
            .and_then(Value::as_u64)
            .ok_or_else(|| "GPUBindGroup.layout must be a native handle".to_string())?;
        let entries = descriptor
            .get("entries")
            .and_then(Value::as_array)
            .ok_or_else(|| "GPUBindGroup.entries must be an array".to_string())?;
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let layout = resources
            .bind_group_layouts
            .get(&layout_id)
            .ok_or_else(|| format!("unknown GPUBindGroupLayout handle {layout_id}"))?;
        let mut native_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let binding = json_u32(entry, "binding", 0);
            let resource = entry
                .get("resource")
                .ok_or_else(|| "GPUBindGroup entry has no resource".to_string())?;
            native_entries.push(wgpu::BindGroupEntry {
                binding,
                resource: binding_resource(resource, &resources)?,
            });
        }
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hyperthree-js-bind-group"),
            layout,
            entries: &native_entries,
        });
        drop(resources);
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .bind_groups
            .insert(id, bind_group);
        Ok(id)
    }

    fn create_render_pipeline(&self, descriptor: &Value) -> Result<u64, String> {
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let layout = descriptor
            .get("layout")
            .and_then(Value::as_u64)
            .map(|id| {
                resources
                    .pipeline_layouts
                    .get(&id)
                    .ok_or_else(|| format!("unknown GPUPipelineLayout handle {id}"))
            })
            .transpose()?;
        let vertex = descriptor
            .get("vertex")
            .ok_or_else(|| "GPURenderPipeline.vertex is required".to_string())?;
        let vertex_module = resources
            .shader_modules
            .get(&json_u64(vertex, "module")?)
            .ok_or_else(|| "unknown vertex GPUShaderModule handle".to_string())?;
        let vertex_buffers_owned = vertex
            .get("buffers")
            .and_then(Value::as_array)
            .map(|buffers| {
                buffers
                    .iter()
                    .map(vertex_buffer_layout_owned)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let vertex_buffers = vertex_buffers_owned
            .iter()
            .map(|layout| wgpu::VertexBufferLayout {
                array_stride: layout.array_stride,
                step_mode: layout.step_mode,
                attributes: &layout.attributes,
            })
            .collect::<Vec<_>>();
        let vertex_entry = json_string(vertex, "entryPoint").unwrap_or_else(|| "main".to_string());
        let fragment = descriptor.get("fragment").filter(|value| !value.is_null());
        let fragment_module = fragment
            .map(|fragment| {
                resources
                    .shader_modules
                    .get(&json_u64(fragment, "module")?)
                    .ok_or_else(|| "unknown fragment GPUShaderModule handle".to_string())
            })
            .transpose()?;
        let fragment_entry = fragment
            .and_then(|value| json_string(value, "entryPoint"))
            .unwrap_or_else(|| "main".to_string());
        let targets = fragment
            .and_then(|value| value.get("targets"))
            .and_then(Value::as_array)
            .map(|targets| {
                targets
                    .iter()
                    .map(color_target_state)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let vertex_state = wgpu::VertexState {
            module: vertex_module,
            entry_point: &vertex_entry,
            buffers: &vertex_buffers,
            compilation_options: Default::default(),
        };
        let fragment_state = fragment_module.map(|module| wgpu::FragmentState {
            module,
            entry_point: &fragment_entry,
            targets: &targets,
            compilation_options: Default::default(),
        });
        let depth_stencil = descriptor
            .get("depthStencil")
            .filter(|value| !value.is_null())
            .map(depth_stencil_state)
            .transpose()?;
        let multisample = descriptor
            .get("multisample")
            .filter(|value| !value.is_null())
            .map(multisample_state)
            .transpose()?
            .unwrap_or_default();
        let primitive = primitive_state(descriptor.get("primitive"));
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hyperthree-js-render-pipeline"),
                layout,
                vertex: vertex_state,
                fragment: fragment_state,
                primitive,
                depth_stencil,
                multisample,
                multiview: None,
            });
        drop(resources);
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .render_pipelines
            .insert(id, pipeline);
        Ok(id)
    }

    fn create_compute_pipeline(&self, descriptor: &Value) -> Result<u64, String> {
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let layout = descriptor
            .get("layout")
            .and_then(Value::as_u64)
            .map(|id| {
                resources
                    .pipeline_layouts
                    .get(&id)
                    .ok_or_else(|| format!("unknown GPUPipelineLayout handle {id}"))
            })
            .transpose()?;
        let compute = descriptor
            .get("compute")
            .ok_or_else(|| "GPUComputePipeline.compute is required".to_string())?;
        let module = resources
            .shader_modules
            .get(&json_u64(compute, "module")?)
            .ok_or_else(|| "unknown compute GPUShaderModule handle".to_string())?;
        let entry_point = json_string(compute, "entryPoint").unwrap_or_else(|| "main".to_string());
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("hyperthree-js-compute-pipeline"),
                layout,
                module,
                entry_point: &entry_point,
                compilation_options: Default::default(),
            });
        drop(resources);
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .compute_pipelines
            .insert(id, pipeline);
        Ok(id)
    }

    fn create_query_set(&self, descriptor: &Value) -> Result<u64, String> {
        let query_type = match json_string(descriptor, "type").as_deref() {
            Some("occlusion") => wgpu::QueryType::Occlusion,
            Some("timestamp") => {
                if !self.features.contains(wgpu::Features::TIMESTAMP_QUERY) {
                    return Err(
                        "timestamp query requested but the native device does not expose timestamp-query"
                            .to_string(),
                    );
                }
                wgpu::QueryType::Timestamp
            }
            Some("pipeline-statistics") => {
                return Err(
                    "pipeline-statistics query sets are not supported by the native bridge"
                        .to_string(),
                )
            }
            Some(other) => return Err(format!("unsupported GPUQuerySet type {other}")),
            None => return Err("GPUQuerySet.type is required".to_string()),
        };
        let count = json_u32(descriptor, "count", 0);
        if count == 0 {
            return Err("GPUQuerySet.count must be greater than zero".to_string());
        }
        let query_set = self.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("hyperthree-js-query-set"),
            ty: query_type,
            count,
        });
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .query_sets
            .insert(id, query_set);
        Ok(id)
    }

    fn destroy_query_set(&self, id: u64) -> Result<(), String> {
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .query_sets
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| format!("unknown GPUQuerySet handle {id}"))
    }

    fn create_render_bundle_encoder(&self, descriptor: &Value) -> Result<u64, String> {
        let color_formats = descriptor
            .get("colorFormats")
            .and_then(Value::as_array)
            .ok_or_else(|| "GPURenderBundleEncoder.colorFormats must be an array".to_string())?
            .iter()
            .map(|value| {
                if value.is_null() {
                    Ok(None)
                } else {
                    value
                        .as_str()
                        .map(|format| Some(texture_format(format)))
                        .ok_or_else(|| {
                            "render bundle color format must be a string or null".to_string()
                        })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let depth_stencil_format = descriptor
            .get("depthStencilFormat")
            .filter(|value| !value.is_null())
            .and_then(Value::as_str)
            .map(|format| wgpu::RenderBundleDepthStencil {
                format: texture_format(format),
                depth_read_only: descriptor
                    .get("depthReadOnly")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                stencil_read_only: descriptor
                    .get("stencilReadOnly")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        let sample_count = json_u32(descriptor, "sampleCount", 1);
        if sample_count == 0 {
            return Err("GPURenderBundleEncoder.sampleCount must be greater than zero".to_string());
        }
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .render_bundle_encoders
            .insert(
                id,
                RenderBundleEncoderState {
                    color_formats,
                    depth_stencil_format,
                    sample_count,
                    commands: Vec::new(),
                },
            );
        Ok(id)
    }

    fn record_render_bundle_command(
        &self,
        encoder_id: u64,
        operation: &str,
        payload: &Value,
    ) -> Result<(), String> {
        let values = payload
            .as_array()
            .ok_or_else(|| "render bundle command payload must be an array".to_string())?;
        let command = match operation {
            "setPipeline" => RenderCommand::SetPipeline(value_u64(values, 0)?),
            "setBindGroup" => RenderCommand::SetBindGroup {
                index: value_u32(values, 0)?,
                bind_group: value_u64(values, 1)?,
                dynamic_offsets: values
                    .get(2)
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .map(|value| {
                        value
                            .as_u64()
                            .map(|value| value as u32)
                            .ok_or_else(|| "dynamic offset must be an integer".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            "setVertexBuffer" => RenderCommand::SetVertexBuffer {
                slot: value_u32(values, 0)?,
                buffer: value_u64(values, 1)?,
                offset: value_u64_or(values, 2, 0)?,
                size: values.get(3).and_then(Value::as_u64),
            },
            "setIndexBuffer" => RenderCommand::SetIndexBuffer {
                buffer: value_u64(values, 0)?,
                offset: value_u64_or(values, 1, 0)?,
                format: values
                    .get(2)
                    .and_then(Value::as_str)
                    .unwrap_or("uint32")
                    .to_string(),
            },
            "draw" => RenderCommand::Draw {
                vertex_count: value_u32(values, 0)?,
                instance_count: value_u32_or(values, 1, 1)?,
                first_vertex: value_u32_or(values, 2, 0)?,
                first_instance: value_u32_or(values, 3, 0)?,
            },
            "drawIndexed" => RenderCommand::DrawIndexed {
                index_count: value_u32(values, 0)?,
                instance_count: value_u32_or(values, 1, 1)?,
                first_index: value_u32_or(values, 2, 0)?,
                base_vertex: value_i32_or(values, 3, 0)?,
                first_instance: value_u32_or(values, 4, 0)?,
            },
            "drawIndirect" | "drawIndexedIndirect" => {
                return Err(format!("{operation} is not supported in render bundles"))
            }
            other => return Err(format!("unsupported render bundle command {other}")),
        };
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        resources
            .render_bundle_encoders
            .get_mut(&encoder_id)
            .ok_or_else(|| format!("unknown GPURenderBundleEncoder handle {encoder_id}"))?
            .commands
            .push(command);
        Ok(())
    }

    fn finish_render_bundle_encoder(&self, encoder_id: u64) -> Result<u64, String> {
        let state = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .render_bundle_encoders
            .remove(&encoder_id)
            .ok_or_else(|| format!("unknown GPURenderBundleEncoder handle {encoder_id}"))?;
        let resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let mut encoder =
            self.device
                .create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some("hyperthree-js-render-bundle-encoder"),
                    color_formats: &state.color_formats,
                    depth_stencil: state.depth_stencil_format,
                    sample_count: state.sample_count,
                    multiview: None,
                });
        for command in state.commands {
            encode_render_bundle_command(&mut encoder, &resources, command)?;
        }
        let bundle = encoder.finish(&wgpu::RenderBundleDescriptor {
            label: Some("hyperthree-js-render-bundle"),
        });
        drop(resources);
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .render_bundles
            .insert(id, bundle);
        Ok(id)
    }

    fn destroy_render_bundle(&self, id: u64) -> Result<(), String> {
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .render_bundles
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| format!("unknown GPURenderBundle handle {id}"))
    }

    fn get_render_pipeline_bind_group_layout(
        &self,
        pipeline_id: u64,
        index: u32,
    ) -> Result<u64, String> {
        let layout = {
            let resources = self
                .resources
                .lock()
                .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
            resources
                .render_pipelines
                .get(&pipeline_id)
                .ok_or_else(|| format!("unknown GPURenderPipeline handle {pipeline_id}"))?
                .get_bind_group_layout(index)
        };
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .bind_group_layouts
            .insert(id, layout);
        Ok(id)
    }

    fn get_compute_pipeline_bind_group_layout(
        &self,
        pipeline_id: u64,
        index: u32,
    ) -> Result<u64, String> {
        let layout = {
            let resources = self
                .resources
                .lock()
                .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
            resources
                .compute_pipelines
                .get(&pipeline_id)
                .ok_or_else(|| format!("unknown GPUComputePipeline handle {pipeline_id}"))?
                .get_bind_group_layout(index)
        };
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .bind_group_layouts
            .insert(id, layout);
        Ok(id)
    }

    fn create_command_encoder(&self) -> Result<u64, String> {
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .command_encoders
            .insert(id, CommandEncoderState::default());
        Ok(id)
    }

    fn begin_render_pass(&self, encoder_id: u64, descriptor: &Value) -> Result<u64, String> {
        let colors = descriptor
            .get("colorAttachments")
            .and_then(Value::as_array)
            .ok_or_else(|| "GPURenderPassDescriptor.colorAttachments must be an array".to_string())?
            .iter()
            .map(color_attachment_state)
            .collect::<Result<Vec<_>, _>>()?;
        let depth_stencil = descriptor
            .get("depthStencilAttachment")
            .filter(|value| !value.is_null())
            .map(depth_stencil_attachment_state)
            .transpose()?;
        let occlusion_query_set = descriptor
            .get("occlusionQuerySet")
            .filter(|value| !value.is_null())
            .and_then(Value::as_u64);
        let timestamp_writes = timestamp_writes_descriptor(descriptor.get("timestampWrites"))?;
        if timestamp_writes.is_some()
            && !self
                .features
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
        {
            return Err(
                "timestampWrites requires the native timestamp-query-inside-passes feature"
                    .to_string(),
            );
        }
        let pass_id = self.allocate_id()?;
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        if !resources.command_encoders.contains_key(&encoder_id) {
            return Err(format!("unknown GPUCommandEncoder handle {encoder_id}"));
        }
        resources.open_passes.insert(
            pass_id,
            OpenPass::Render {
                encoder: encoder_id,
                state: RenderPassState {
                    colors,
                    depth_stencil,
                    occlusion_query_set,
                    timestamp_writes,
                    commands: Vec::new(),
                },
            },
        );
        Ok(pass_id)
    }

    fn begin_compute_pass(&self, encoder_id: u64, descriptor: &Value) -> Result<u64, String> {
        let timestamp_writes = timestamp_writes_descriptor(descriptor.get("timestampWrites"))?;
        if timestamp_writes.is_some()
            && !self
                .features
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
        {
            return Err(
                "timestampWrites requires the native timestamp-query-inside-passes feature"
                    .to_string(),
            );
        }
        let pass_id = self.allocate_id()?;
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        if !resources.command_encoders.contains_key(&encoder_id) {
            return Err(format!("unknown GPUCommandEncoder handle {encoder_id}"));
        }
        resources.open_passes.insert(
            pass_id,
            OpenPass::Compute {
                encoder: encoder_id,
                state: ComputePassState {
                    timestamp_writes,
                    commands: Vec::new(),
                },
            },
        );
        Ok(pass_id)
    }

    fn end_pass(&self, pass_id: u64) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let pass = resources
            .open_passes
            .remove(&pass_id)
            .ok_or_else(|| format!("unknown or already ended GPURenderPass handle {pass_id}"))?;
        let (encoder_id, command) = match pass {
            OpenPass::Render { encoder, state } => (encoder, RecordedCommand::RenderPass(state)),
            OpenPass::Compute { encoder, state } => (encoder, RecordedCommand::ComputePass(state)),
        };
        let encoder = resources
            .command_encoders
            .get_mut(&encoder_id)
            .ok_or_else(|| format!("unknown GPUCommandEncoder handle {encoder_id}"))?;
        encoder.commands.push(command);
        Ok(())
    }

    fn push_render_command(&self, pass_id: u64, command: RenderCommand) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        match resources.open_passes.get_mut(&pass_id) {
            Some(OpenPass::Render { state, .. }) => {
                state.commands.push(command);
                Ok(())
            }
            Some(OpenPass::Compute { .. }) => Err(format!(
                "GPUComputePass handle {pass_id} cannot receive render commands"
            )),
            None => Err(format!("unknown or ended GPURenderPass handle {pass_id}")),
        }
    }

    fn push_compute_command(&self, pass_id: u64, command: ComputeCommand) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        match resources.open_passes.get_mut(&pass_id) {
            Some(OpenPass::Compute { state, .. }) => {
                state.commands.push(command);
                Ok(())
            }
            Some(OpenPass::Render { .. }) => Err(format!(
                "GPURenderPass handle {pass_id} cannot receive compute commands"
            )),
            None => Err(format!("unknown or ended GPUComputePass handle {pass_id}")),
        }
    }

    fn record_encoder_command(
        &self,
        encoder_id: u64,
        operation: &str,
        payload: &Value,
    ) -> Result<(), String> {
        let command = match operation {
            "clearBuffer" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "clearBuffer payload must be an array".to_string())?;
                RecordedCommand::ClearBuffer {
                    buffer: value_u64(values, 0)?,
                    offset: value_u64_or(values, 1, 0)?,
                    size: values.get(2).and_then(Value::as_u64),
                }
            }
            "copyBufferToBuffer" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "copyBufferToBuffer payload must be an array".to_string())?;
                RecordedCommand::CopyBufferToBuffer {
                    source: value_u64(values, 0)?,
                    source_offset: value_u64_or(values, 1, 0)?,
                    destination: value_u64(values, 2)?,
                    destination_offset: value_u64_or(values, 3, 0)?,
                    size: value_u64(values, 4)?,
                }
            }
            "copyBufferToTexture" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "copyBufferToTexture payload must be an array".to_string())?;
                let source = values
                    .first()
                    .and_then(Value::as_object)
                    .ok_or_else(|| "buffer texture source must be an object".to_string())?;
                let destination = texture_copy_descriptor(values, 1)?;
                let size = values
                    .get(2)
                    .and_then(Value::as_array)
                    .ok_or_else(|| "copy size must be an array".to_string())?;
                RecordedCommand::CopyBufferToTexture {
                    source: source
                        .get("buffer")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "buffer texture source has no buffer handle".to_string())?,
                    source_offset: source.get("offset").and_then(Value::as_u64).unwrap_or(0),
                    bytes_per_row: source
                        .get("bytesPerRow")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "buffer texture source has no bytesPerRow".to_string())?
                        as u32,
                    rows_per_image: source
                        .get("rowsPerImage")
                        .and_then(Value::as_u64)
                        .unwrap_or(value_u32_or(size, 1, 1)? as u64)
                        as u32,
                    destination: destination.0,
                    destination_mip_level: destination.1,
                    destination_origin: destination.2,
                    size: [
                        value_u32(size, 0)?,
                        value_u32_or(size, 1, 1)?,
                        value_u32_or(size, 2, 1)?,
                    ],
                }
            }
            "copyTextureToTexture" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "copyTextureToTexture payload must be an array".to_string())?;
                let source = texture_copy_descriptor(values, 0)?;
                let destination = texture_copy_descriptor(values, 1)?;
                let size = values
                    .get(2)
                    .and_then(Value::as_array)
                    .ok_or_else(|| "copy size must be an array".to_string())?;
                RecordedCommand::CopyTextureToTexture {
                    source: source.0,
                    source_mip_level: source.1,
                    source_origin: source.2,
                    destination: destination.0,
                    destination_mip_level: destination.1,
                    destination_origin: destination.2,
                    size: [
                        value_u32(size, 0)?,
                        value_u32_or(size, 1, 1)?,
                        value_u32_or(size, 2, 1)?,
                    ],
                }
            }
            "copyTextureToBuffer" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "copyTextureToBuffer payload must be an array".to_string())?;
                let source = texture_copy_descriptor(values, 0)?;
                let destination = values
                    .get(1)
                    .and_then(Value::as_object)
                    .ok_or_else(|| "texture buffer destination must be an object".to_string())?;
                let size = values
                    .get(2)
                    .and_then(Value::as_array)
                    .ok_or_else(|| "copy size must be an array".to_string())?;
                RecordedCommand::CopyTextureToBuffer {
                    source: source.0,
                    source_mip_level: source.1,
                    source_origin: source.2,
                    destination: destination
                        .get("buffer")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            "texture buffer destination has no buffer handle".to_string()
                        })?,
                    destination_offset: destination
                        .get("offset")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    bytes_per_row: destination
                        .get("bytesPerRow")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            "texture buffer destination has no bytesPerRow".to_string()
                        })? as u32,
                    rows_per_image: destination
                        .get("rowsPerImage")
                        .and_then(Value::as_u64)
                        .unwrap_or(value_u32_or(size, 1, 1)? as u64)
                        as u32,
                    size: [
                        value_u32(size, 0)?,
                        value_u32_or(size, 1, 1)?,
                        value_u32_or(size, 2, 1)?,
                    ],
                }
            }
            "resolveQuerySet" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "resolveQuerySet payload must be an array".to_string())?;
                RecordedCommand::ResolveQuerySet {
                    query_set: value_u64(values, 0)?,
                    first_query: value_u32_or(values, 1, 0)?,
                    query_count: value_u32(values, 2)?,
                    destination: value_u64(values, 3)?,
                    destination_offset: value_u64_or(values, 4, 0)?,
                }
            }
            "writeTimestamp" => {
                let values = payload
                    .as_array()
                    .ok_or_else(|| "writeTimestamp payload must be an array".to_string())?;
                RecordedCommand::WriteTimestamp {
                    query_set: value_u64(values, 0)?,
                    query_index: value_u32(values, 1)?,
                }
            }
            _ => return Err(format!("unsupported WebGPU encoder command {operation}")),
        };
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let encoder = resources
            .command_encoders
            .get_mut(&encoder_id)
            .ok_or_else(|| format!("unknown GPUCommandEncoder handle {encoder_id}"))?;
        encoder.commands.push(command);
        Ok(())
    }

    fn record_pass_command(
        &self,
        pass_id: u64,
        operation: &str,
        payload: &Value,
    ) -> Result<(), String> {
        let values = payload
            .as_array()
            .ok_or_else(|| "WebGPU pass command payload must be an array".to_string())?;
        match operation {
            "setPipeline" => {
                let id = value_u64(values, 0)?;
                let resources = self
                    .resources
                    .lock()
                    .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
                if resources.render_pipelines.contains_key(&id) {
                    drop(resources);
                    self.push_render_command(pass_id, RenderCommand::SetPipeline(id))
                } else if resources.compute_pipelines.contains_key(&id) {
                    drop(resources);
                    self.push_compute_command(pass_id, ComputeCommand::SetPipeline(id))
                } else {
                    Err(format!("unknown pipeline handle {id}"))
                }
            }
            "setBindGroup" => {
                let index = value_u32(values, 0)?;
                let bind_group = value_u64(values, 1)?;
                let dynamic_offsets = values
                    .get(2)
                    .and_then(Value::as_array)
                    .ok_or_else(|| "dynamic offsets must be an array".to_string())?
                    .iter()
                    .map(|value| {
                        value
                            .as_u64()
                            .map(|value| value as u32)
                            .ok_or_else(|| "dynamic offset must be an integer".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let is_render = self
                    .resources
                    .lock()
                    .map_err(|_| "WebGPU resource registry poisoned".to_string())?
                    .open_passes
                    .get(&pass_id)
                    .map(|pass| matches!(pass, OpenPass::Render { .. }))
                    .ok_or_else(|| format!("unknown or ended pass handle {pass_id}"))?;
                if is_render {
                    self.push_render_command(
                        pass_id,
                        RenderCommand::SetBindGroup {
                            index,
                            bind_group,
                            dynamic_offsets,
                        },
                    )
                } else {
                    self.push_compute_command(
                        pass_id,
                        ComputeCommand::SetBindGroup {
                            index,
                            bind_group,
                            dynamic_offsets,
                        },
                    )
                }
            }
            "setVertexBuffer" => self.push_render_command(
                pass_id,
                RenderCommand::SetVertexBuffer {
                    slot: value_u32(values, 0)?,
                    buffer: value_u64(values, 1)?,
                    offset: value_u64_or(values, 2, 0)?,
                    size: values.get(3).and_then(Value::as_u64),
                },
            ),
            "setIndexBuffer" => self.push_render_command(
                pass_id,
                RenderCommand::SetIndexBuffer {
                    buffer: value_u64(values, 0)?,
                    offset: value_u64_or(values, 1, 0)?,
                    format: values
                        .get(2)
                        .and_then(Value::as_str)
                        .unwrap_or("uint32")
                        .to_string(),
                },
            ),
            "setViewport" => self.push_render_command(
                pass_id,
                RenderCommand::SetViewport {
                    x: value_f32(values, 0)?,
                    y: value_f32(values, 1)?,
                    width: value_f32(values, 2)?,
                    height: value_f32(values, 3)?,
                    min_depth: value_f32(values, 4)?,
                    max_depth: value_f32(values, 5)?,
                },
            ),
            "setScissorRect" => self.push_render_command(
                pass_id,
                RenderCommand::SetScissorRect {
                    x: value_u32(values, 0)?,
                    y: value_u32(values, 1)?,
                    width: value_u32(values, 2)?,
                    height: value_u32(values, 3)?,
                },
            ),
            "setBlendConstant" => self.push_render_command(
                pass_id,
                RenderCommand::SetBlendConstant([
                    value_f64(values, 0)?,
                    value_f64(values, 1)?,
                    value_f64(values, 2)?,
                    value_f64(values, 3)?,
                ]),
            ),
            "setStencilReference" => self.push_render_command(
                pass_id,
                RenderCommand::SetStencilReference(value_u32(values, 0)?),
            ),
            "draw" => self.push_render_command(
                pass_id,
                RenderCommand::Draw {
                    vertex_count: value_u32(values, 0)?,
                    instance_count: value_u32_or(values, 1, 1)?,
                    first_vertex: value_u32_or(values, 2, 0)?,
                    first_instance: value_u32_or(values, 3, 0)?,
                },
            ),
            "drawIndexed" => self.push_render_command(
                pass_id,
                RenderCommand::DrawIndexed {
                    index_count: value_u32(values, 0)?,
                    instance_count: value_u32_or(values, 1, 1)?,
                    first_index: value_u32_or(values, 2, 0)?,
                    base_vertex: value_i32_or(values, 3, 0)?,
                    first_instance: value_u32_or(values, 4, 0)?,
                },
            ),
            "drawIndirect" => self.push_render_command(
                pass_id,
                RenderCommand::DrawIndirect {
                    buffer: value_u64(values, 0)?,
                    offset: value_u64_or(values, 1, 0)?,
                },
            ),
            "drawIndexedIndirect" => self.push_render_command(
                pass_id,
                RenderCommand::DrawIndexedIndirect {
                    buffer: value_u64(values, 0)?,
                    offset: value_u64_or(values, 1, 0)?,
                },
            ),
            "beginOcclusionQuery" => self.push_render_command(
                pass_id,
                RenderCommand::BeginOcclusionQuery(value_u32(values, 0)?),
            ),
            "endOcclusionQuery" => {
                self.push_render_command(pass_id, RenderCommand::EndOcclusionQuery)
            }
            "executeBundles" => self.push_render_command(
                pass_id,
                RenderCommand::ExecuteBundles(
                    values
                        .iter()
                        .map(|value| {
                            value.as_u64().ok_or_else(|| {
                                "render bundle handle must be an integer".to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            ),
            "dispatchWorkgroups" => self.push_compute_command(
                pass_id,
                ComputeCommand::DispatchWorkgroups {
                    x: value_u32(values, 0)?,
                    y: value_u32_or(values, 1, 1)?,
                    z: value_u32_or(values, 2, 1)?,
                },
            ),
            "dispatchWorkgroupsIndirect" => self.push_compute_command(
                pass_id,
                ComputeCommand::DispatchWorkgroupsIndirect {
                    buffer: value_u64(values, 0)?,
                    offset: value_u64_or(values, 1, 0)?,
                },
            ),
            _ => Err(format!("unsupported WebGPU pass command {operation}")),
        }
    }

    fn finish_command_encoder(&self, encoder_id: u64) -> Result<u64, String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        if resources.open_passes.values().any(|pass| match pass {
            OpenPass::Render { encoder, .. } | OpenPass::Compute { encoder, .. } => {
                *encoder == encoder_id
            }
        }) {
            return Err("GPUCommandEncoder has an open pass".to_string());
        }
        let state = resources
            .command_encoders
            .remove(&encoder_id)
            .ok_or_else(|| format!("unknown GPUCommandEncoder handle {encoder_id}"))?;
        let surface_textures = collect_surface_texture_ids(&state.commands, &resources);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("hyperthree-js-command-encoder"),
            });
        for command in state.commands {
            match command {
                RecordedCommand::RenderPass(pass) => {
                    encode_render_pass(&mut encoder, &resources, pass)?;
                }
                RecordedCommand::ComputePass(pass) => {
                    encode_compute_pass(&mut encoder, &resources, pass)?;
                }
                RecordedCommand::ClearBuffer {
                    buffer,
                    offset,
                    size,
                } => {
                    let buffer = resources
                        .buffers
                        .get(&buffer)
                        .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
                    encoder.clear_buffer(buffer, offset, size);
                }
                RecordedCommand::CopyBufferToBuffer {
                    source,
                    source_offset,
                    destination,
                    destination_offset,
                    size,
                } => {
                    let source_buffer = resources
                        .buffers
                        .get(&source)
                        .ok_or_else(|| format!("unknown source GPUBuffer handle {source}"))?;
                    let destination_buffer =
                        resources.buffers.get(&destination).ok_or_else(|| {
                            format!("unknown destination GPUBuffer handle {destination}")
                        })?;
                    encoder.copy_buffer_to_buffer(
                        source_buffer,
                        source_offset,
                        destination_buffer,
                        destination_offset,
                        size,
                    );
                }
                RecordedCommand::CopyBufferToTexture {
                    source,
                    source_offset,
                    bytes_per_row,
                    rows_per_image,
                    destination,
                    destination_mip_level,
                    destination_origin,
                    size,
                } => {
                    let source_buffer = resources
                        .buffers
                        .get(&source)
                        .ok_or_else(|| format!("unknown source GPUBuffer handle {source}"))?;
                    let destination_texture = texture_resource(&resources, destination)?;
                    encoder.copy_buffer_to_texture(
                        wgpu::ImageCopyBuffer {
                            buffer: source_buffer,
                            layout: wgpu::ImageDataLayout {
                                offset: source_offset,
                                bytes_per_row: Some(bytes_per_row),
                                rows_per_image: Some(rows_per_image),
                            },
                        },
                        wgpu::ImageCopyTexture {
                            texture: destination_texture,
                            mip_level: destination_mip_level,
                            origin: wgpu::Origin3d {
                                x: destination_origin[0],
                                y: destination_origin[1],
                                z: destination_origin[2],
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: size[2],
                        },
                    );
                }
                RecordedCommand::CopyTextureToTexture {
                    source,
                    source_mip_level,
                    source_origin,
                    destination,
                    destination_mip_level,
                    destination_origin,
                    size,
                } => {
                    let source_texture = texture_resource(&resources, source)?;
                    let destination_texture = texture_resource(&resources, destination)?;
                    encoder.copy_texture_to_texture(
                        wgpu::ImageCopyTexture {
                            texture: source_texture,
                            mip_level: source_mip_level,
                            origin: wgpu::Origin3d {
                                x: source_origin[0],
                                y: source_origin[1],
                                z: source_origin[2],
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::ImageCopyTexture {
                            texture: destination_texture,
                            mip_level: destination_mip_level,
                            origin: wgpu::Origin3d {
                                x: destination_origin[0],
                                y: destination_origin[1],
                                z: destination_origin[2],
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: size[2],
                        },
                    );
                }
                RecordedCommand::CopyTextureToBuffer {
                    source,
                    source_mip_level,
                    source_origin,
                    destination,
                    destination_offset,
                    bytes_per_row,
                    rows_per_image,
                    size,
                } => {
                    let source_texture = texture_resource(&resources, source)?;
                    let destination_buffer =
                        resources.buffers.get(&destination).ok_or_else(|| {
                            format!("unknown destination GPUBuffer handle {destination}")
                        })?;
                    encoder.copy_texture_to_buffer(
                        wgpu::ImageCopyTexture {
                            texture: source_texture,
                            mip_level: source_mip_level,
                            origin: wgpu::Origin3d {
                                x: source_origin[0],
                                y: source_origin[1],
                                z: source_origin[2],
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::ImageCopyBuffer {
                            buffer: destination_buffer,
                            layout: wgpu::ImageDataLayout {
                                offset: destination_offset,
                                bytes_per_row: Some(bytes_per_row),
                                rows_per_image: Some(rows_per_image),
                            },
                        },
                        wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: size[2],
                        },
                    );
                }
                RecordedCommand::ResolveQuerySet {
                    query_set,
                    first_query,
                    query_count,
                    destination,
                    destination_offset,
                } => {
                    let query_set = resources
                        .query_sets
                        .get(&query_set)
                        .ok_or_else(|| format!("unknown GPUQuerySet handle {query_set}"))?;
                    let destination = resources.buffers.get(&destination).ok_or_else(|| {
                        format!("unknown query destination GPUBuffer handle {destination}")
                    })?;
                    encoder.resolve_query_set(
                        query_set,
                        first_query..first_query + query_count,
                        destination,
                        destination_offset,
                    );
                }
                RecordedCommand::WriteTimestamp {
                    query_set,
                    query_index,
                } => {
                    let query_set = resources.query_sets.get(&query_set).ok_or_else(|| {
                        format!("unknown timestamp GPUQuerySet handle {query_set}")
                    })?;
                    encoder.write_timestamp(query_set, query_index);
                }
            }
        }
        let command_buffer = encoder.finish();
        drop(resources);
        let id = self.allocate_id()?;
        self.resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?
            .command_buffers
            .insert(
                id,
                QueuedCommandBuffer {
                    buffer: command_buffer,
                    surface_textures,
                },
            );
        Ok(id)
    }

    fn submit(&self, command_buffer_ids: &[u64]) -> Result<(), String> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "WebGPU resource registry poisoned".to_string())?;
        let mut command_buffers = Vec::with_capacity(command_buffer_ids.len());
        let mut surface_texture_ids = Vec::new();
        for id in command_buffer_ids {
            let command_buffer = resources
                .command_buffers
                .remove(id)
                .ok_or_else(|| format!("unknown GPUCommandBuffer handle {id}"))?;
            command_buffers.push(command_buffer.buffer);
            surface_texture_ids.extend(command_buffer.surface_textures);
        }
        self.queue.submit(command_buffers);
        surface_texture_ids.sort_unstable();
        surface_texture_ids.dedup();
        for texture_id in surface_texture_ids {
            if let Some(surface_texture) = resources.surface_textures.remove(&texture_id) {
                surface_texture.present();
                remove_surface_texture_views(&mut resources, texture_id);
                self.presented_this_frame.store(true, Ordering::Release);
            }
        }
        Ok(())
    }
}

pub fn register_bindings(
    context: &mut Context,
    gpu: Option<SharedNativeWebGpuContext>,
) -> Result<()> {
    let Some(gpu) = gpu else {
        return Ok(());
    };
    let feature_names = gpu
        .webgpu_feature_names()
        .into_iter()
        .map(|feature| format!("'{feature}'"))
        .collect::<Vec<_>>()
        .join(", ");

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

    let read_buffer_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuReadBuffer",
        3,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            let offset = number_arg(args, 1, context)? as u64;
            let size = number_arg(args, 2, context)? as u64;
            let bytes = read_buffer_gpu
                .read_buffer(id, offset, size)
                .map_err(native_error)?;
            let block = AlignedVec::from_iter(0, bytes);
            JsArrayBuffer::from_byte_block(block, context)
                .map(Into::into)
                .map_err(|error| {
                    native_error(format!("failed to create readback ArrayBuffer: {error}"))
                })
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
        8,
        move |_this, args, context| {
            let width = number_arg(args, 0, context)? as u32;
            let height = number_arg(args, 1, context)? as u32;
            let depth = optional_number_arg(args, 2, context)?.unwrap_or(1.0) as u32;
            let format = string_arg(args, 3, context)?;
            let usage = number_arg(args, 4, context)? as u64;
            let mip_level_count = optional_number_arg(args, 5, context)?.unwrap_or(1.0) as u32;
            let sample_count = optional_number_arg(args, 6, context)?.unwrap_or(1.0) as u32;
            let dimension =
                optional_string_arg(args, 7, context)?.unwrap_or_else(|| "2d".to_string());
            create_texture_gpu
                .create_texture(TextureCreateInfo {
                    width,
                    height,
                    depth,
                    format: &format,
                    usage,
                    mip_level_count,
                    sample_count,
                    dimension: &dimension,
                })
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let destroy_texture_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDestroyTexture",
        1,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            destroy_texture_gpu
                .destroy_texture(id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let write_texture_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuWriteTexture",
        9,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            let width = number_arg(args, 1, context)? as u32;
            let height = number_arg(args, 2, context)? as u32;
            let depth = optional_number_arg(args, 3, context)?.unwrap_or(1.0) as u32;
            let bytes_per_row =
                optional_number_arg(args, 4, context)?.unwrap_or((width * 4) as f64) as u32;
            let rows_per_image =
                optional_number_arg(args, 5, context)?.unwrap_or(height as f64) as u32;
            let origin = optional_u32_array_arg(args, 6, context)?.unwrap_or([0, 0, 0]);
            let mip_level = optional_number_arg(args, 7, context)?.unwrap_or(0.0) as u32;
            let bytes = byte_array_arg(args, 8, context)?;
            write_texture_gpu
                .write_texture(
                    TextureWriteInfo {
                        id,
                        mip_level,
                        width,
                        height,
                        depth,
                        bytes_per_row,
                        rows_per_image,
                        origin,
                    },
                    &bytes,
                )
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

    let texture_view_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuCreateTextureView",
        1,
        move |_this, args, context| {
            let texture_id = number_arg(args, 0, context)? as u64;
            let descriptor = if args.len() > 1 {
                json_arg(args, 1, context)?
            } else {
                Value::Null
            };
            texture_view_gpu
                .create_texture_view(texture_id, &descriptor)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let current_texture_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuGetCurrentTexture",
        0,
        move |_this, _args, _context| {
            current_texture_gpu
                .get_current_surface_texture()
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let discard_texture_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDiscardSurfaceTexture",
        1,
        move |_this, args, context| {
            let texture_id = number_arg(args, 0, context)? as u64;
            discard_texture_gpu
                .discard_surface_texture(texture_id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let resize_canvas_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuResizeCanvas",
        2,
        move |_this, args, context| {
            let width = number_arg(args, 0, context)? as u32;
            let height = number_arg(args, 1, context)? as u32;
            resize_canvas_gpu
                .resize_surface(width, height)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let configure_canvas_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuConfigureCanvas",
        1,
        move |_this, args, context| {
            let descriptor = json_arg(args, 0, context)?;
            configure_canvas_gpu
                .configure_surface(&descriptor)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let canvas_configuration_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuGetCanvasConfiguration",
        0,
        move |_this, _args, _context| {
            canvas_configuration_gpu
                .canvas_configuration()
                .and_then(|configuration| {
                    serde_json::to_string(&configuration).map_err(|error| error.to_string())
                })
                .map(|configuration| JsValue::from(js_string!(configuration)))
                .map_err(native_error)
        },
    )?;

    let unconfigure_canvas_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuUnconfigureCanvas",
        0,
        move |_this, _args, _context| {
            unconfigure_canvas_gpu
                .unconfigure_surface()
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let poll_device_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuPollDeviceLost",
        0,
        move |_this, _args, _context| {
            Ok(poll_device_gpu
                .poll_device_lost()
                .map(|event| JsValue::from(js_string!(event)))
                .unwrap_or_else(JsValue::null))
        },
    )?;

    let destroy_device_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDestroyDevice",
        0,
        move |_this, _args, _context| {
            destroy_device_gpu.destroy_device();
            Ok(JsValue::undefined())
        },
    )?;

    let push_error_scope_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuPushErrorScope",
        1,
        move |_this, args, context| {
            let filter = string_arg(args, 0, context)?;
            push_error_scope_gpu
                .push_error_scope(&filter)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let pop_error_scope_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuPopErrorScope",
        0,
        move |_this, _args, _context| {
            Ok(pop_error_scope_gpu
                .pop_error_scope()
                .map(|error| JsValue::from(js_string!(error)))
                .unwrap_or_else(JsValue::null))
        },
    )?;

    let sampler_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateSampler",
        sampler_gpu,
        |gpu, descriptor| gpu.create_sampler(descriptor),
    )?;

    let destroy_sampler_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDestroySampler",
        1,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            destroy_sampler_gpu
                .destroy_sampler(id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let bind_group_layout_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateBindGroupLayout",
        bind_group_layout_gpu,
        |gpu, descriptor| gpu.create_bind_group_layout(descriptor),
    )?;

    let pipeline_layout_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreatePipelineLayout",
        pipeline_layout_gpu,
        |gpu, descriptor| gpu.create_pipeline_layout(descriptor),
    )?;

    let bind_group_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateBindGroup",
        bind_group_gpu,
        |gpu, descriptor| gpu.create_bind_group(descriptor),
    )?;

    let render_pipeline_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateRenderPipeline",
        render_pipeline_gpu,
        |gpu, descriptor| gpu.create_render_pipeline(descriptor),
    )?;

    let compute_pipeline_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateComputePipeline",
        compute_pipeline_gpu,
        |gpu, descriptor| gpu.create_compute_pipeline(descriptor),
    )?;

    let query_set_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateQuerySet",
        query_set_gpu,
        |gpu, descriptor| gpu.create_query_set(descriptor),
    )?;

    let destroy_query_set_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDestroyQuerySet",
        1,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            destroy_query_set_gpu
                .destroy_query_set(id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let bundle_encoder_gpu = gpu.clone();
    register_json_resource(
        context,
        "__hyperthreeWebGpuCreateRenderBundleEncoder",
        bundle_encoder_gpu,
        |gpu, descriptor| gpu.create_render_bundle_encoder(descriptor),
    )?;

    let bundle_command_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuRenderBundleCommand",
        3,
        move |_this, args, context| {
            let encoder_id = number_arg(args, 0, context)? as u64;
            let operation = string_arg(args, 1, context)?;
            let payload = json_arg(args, 2, context)?;
            bundle_command_gpu
                .record_render_bundle_command(encoder_id, &operation, &payload)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let finish_bundle_encoder_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuFinishRenderBundleEncoder",
        1,
        move |_this, args, context| {
            let encoder_id = number_arg(args, 0, context)? as u64;
            finish_bundle_encoder_gpu
                .finish_render_bundle_encoder(encoder_id)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let destroy_render_bundle_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuDestroyRenderBundle",
        1,
        move |_this, args, context| {
            let id = number_arg(args, 0, context)? as u64;
            destroy_render_bundle_gpu
                .destroy_render_bundle(id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let render_layout_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuGetRenderPipelineBindGroupLayout",
        2,
        move |_this, args, context| {
            let pipeline_id = number_arg(args, 0, context)? as u64;
            let index = number_arg(args, 1, context)? as u32;
            render_layout_gpu
                .get_render_pipeline_bind_group_layout(pipeline_id, index)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let compute_layout_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuGetComputePipelineBindGroupLayout",
        2,
        move |_this, args, context| {
            let pipeline_id = number_arg(args, 0, context)? as u64;
            let index = number_arg(args, 1, context)? as u32;
            compute_layout_gpu
                .get_compute_pipeline_bind_group_layout(pipeline_id, index)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let encoder_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuCreateCommandEncoder",
        0,
        move |_this, _args, _context| {
            encoder_gpu
                .create_command_encoder()
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let begin_render_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuBeginRenderPass",
        2,
        move |_this, args, context| {
            let encoder_id = number_arg(args, 0, context)? as u64;
            let descriptor = json_arg(args, 1, context)?;
            begin_render_gpu
                .begin_render_pass(encoder_id, &descriptor)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let begin_compute_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuBeginComputePass",
        2,
        move |_this, args, context| {
            let encoder_id = number_arg(args, 0, context)? as u64;
            let descriptor = json_arg(args, 1, context)?;
            begin_compute_gpu
                .begin_compute_pass(encoder_id, &descriptor)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let pass_command_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuPassCommand",
        3,
        move |_this, args, context| {
            let pass_id = number_arg(args, 0, context)? as u64;
            let operation = string_arg(args, 1, context)?;
            let payload = json_arg(args, 2, context)?;
            pass_command_gpu
                .record_pass_command(pass_id, &operation, &payload)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let encoder_command_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuEncoderCommand",
        3,
        move |_this, args, context| {
            let encoder_id = number_arg(args, 0, context)? as u64;
            let operation = string_arg(args, 1, context)?;
            let payload = json_arg(args, 2, context)?;
            encoder_command_gpu
                .record_encoder_command(encoder_id, &operation, &payload)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let end_pass_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuEndPass",
        1,
        move |_this, args, context| {
            let pass_id = number_arg(args, 0, context)? as u64;
            end_pass_gpu
                .end_pass(pass_id)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let finish_encoder_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuFinishCommandEncoder",
        1,
        move |_this, args, context| {
            let encoder_id = number_arg(args, 0, context)? as u64;
            finish_encoder_gpu
                .finish_command_encoder(encoder_id)
                .map(JsValue::from)
                .map_err(native_error)
        },
    )?;

    let submit_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuSubmit",
        1,
        move |_this, args, context| {
            let ids = json_arg(args, 0, context)?
                .as_array()
                .ok_or_else(|| {
                    JsNativeError::typ().with_message("command buffers must be an array")
                })?
                .iter()
                .map(|value| {
                    value.as_u64().ok_or_else(|| {
                        JsNativeError::typ()
                            .with_message("command buffer handle must be an integer")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            submit_gpu
                .submit(&ids)
                .map(|_| JsValue::undefined())
                .map_err(native_error)
        },
    )?;

    let wait_gpu = gpu.clone();
    register(
        context,
        "__hyperthreeWebGpuWaitForSubmittedWork",
        0,
        move |_this, _args, _context| {
            wait_gpu.wait_for_submitted_work();
            Ok(JsValue::undefined())
        },
    )?;

    let limits = serde_json::to_string(&gpu.webgpu_limits())
        .map_err(|error| anyhow::anyhow!("failed to serialize WebGPU limits: {error}"))?;
    let bootstrap = WEBGPU_BOOTSTRAP
        .replace("__HYPERTHREE_FEATURES__", &feature_names)
        .replace("__HYPERTHREE_LIMITS__", &limits);
    context
        .eval(boa_engine::Source::from_bytes(bootstrap.as_bytes()))
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

fn register_json_resource<F>(
    context: &mut Context,
    name: &str,
    gpu: SharedNativeWebGpuContext,
    create: F,
) -> Result<()>
where
    F: Fn(&NativeWebGpuContext, &Value) -> Result<u64, String> + 'static,
{
    register(context, name, 1, move |_this, args, context| {
        let descriptor = json_arg(args, 0, context)?;
        create(&gpu, &descriptor)
            .map(JsValue::from)
            .map_err(native_error)
    })
}

fn json_arg(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<Value> {
    let source = string_arg(args, index, context)?;
    serde_json::from_str(&source).map_err(|error| {
        JsNativeError::syntax()
            .with_message(format!("invalid WebGPU descriptor JSON: {error}"))
            .into()
    })
}

fn collect_surface_texture_ids(commands: &[RecordedCommand], resources: &Resources) -> Vec<u64> {
    let mut ids = Vec::new();
    for command in commands {
        if let RecordedCommand::RenderPass(pass) = command {
            for attachment in &pass.colors {
                collect_surface_texture_id(attachment.view, resources, &mut ids);
                if let Some(resolve_target) = attachment.resolve_target {
                    collect_surface_texture_id(resolve_target, resources, &mut ids);
                }
            }
            if let Some(depth_stencil) = &pass.depth_stencil {
                collect_surface_texture_id(depth_stencil.view, resources, &mut ids);
            }
        }
    }
    ids
}

fn remove_surface_texture_views(resources: &mut Resources, texture_id: u64) {
    let stale_views = resources
        .texture_view_sources
        .iter()
        .filter_map(|(view_id, source_id)| (*source_id == texture_id).then_some(*view_id))
        .collect::<Vec<_>>();
    for view_id in stale_views {
        resources.texture_view_sources.remove(&view_id);
        resources.texture_views.remove(&view_id);
    }
}

fn texture_resource(resources: &Resources, id: u64) -> Result<&wgpu::Texture, String> {
    if let Some(texture) = resources.textures.get(&id) {
        return Ok(texture);
    }
    resources
        .surface_textures
        .get(&id)
        .map(|surface_texture| &surface_texture.texture)
        .ok_or_else(|| format!("unknown GPUTexture handle {id}"))
}

fn texture_copy_descriptor(values: &[Value], index: usize) -> Result<(u64, u32, [u32; 3]), String> {
    let descriptor = values
        .get(index)
        .and_then(Value::as_object)
        .ok_or_else(|| "texture copy descriptor must be an object".to_string())?;
    let origin = descriptor
        .get("origin")
        .and_then(Value::as_array)
        .map(|origin| {
            Ok::<[u32; 3], String>([
                value_u32(origin, 0)?,
                value_u32_or(origin, 1, 0)?,
                value_u32_or(origin, 2, 0)?,
            ])
        })
        .transpose()?
        .unwrap_or([0, 0, 0]);
    Ok((
        descriptor
            .get("texture")
            .and_then(Value::as_u64)
            .ok_or_else(|| "texture copy descriptor has no texture handle".to_string())?,
        descriptor
            .get("mipLevel")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        origin,
    ))
}

fn timestamp_writes_descriptor(value: Option<&Value>) -> Result<Option<TimestampWrites>, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    Ok(Some(TimestampWrites {
        query_set: value
            .get("querySet")
            .and_then(Value::as_u64)
            .ok_or_else(|| "timestampWrites.querySet is required".to_string())?,
        beginning_of_pass_write_index: value
            .get("beginningOfPassWriteIndex")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        end_of_pass_write_index: value
            .get("endOfPassWriteIndex")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
    }))
}

fn collect_surface_texture_id(view_id: u64, resources: &Resources, ids: &mut Vec<u64>) {
    if let Some(texture_id) = resources.texture_view_sources.get(&view_id) {
        if resources.surface_textures.contains_key(texture_id) && !ids.contains(texture_id) {
            ids.push(*texture_id);
        }
    }
}

fn encode_render_pass(
    encoder: &mut wgpu::CommandEncoder,
    resources: &Resources,
    state: RenderPassState,
) -> Result<(), String> {
    let color_attachments = state
        .colors
        .iter()
        .map(|attachment| {
            let view = resources
                .texture_views
                .get(&attachment.view)
                .ok_or_else(|| format!("unknown GPUTextureView handle {}", attachment.view))?;
            let resolve_target = attachment
                .resolve_target
                .map(|id| {
                    resources
                        .texture_views
                        .get(&id)
                        .ok_or_else(|| format!("unknown resolve GPUTextureView handle {id}"))
                })
                .transpose()?;
            Ok(Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target,
                ops: wgpu::Operations {
                    load: color_load_op(&attachment.load_op, attachment.clear_value),
                    store: store_op(&attachment.store_op),
                },
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let depth_stencil_attachment = state
        .depth_stencil
        .as_ref()
        .map(|attachment| {
            let view = resources
                .texture_views
                .get(&attachment.view)
                .ok_or_else(|| {
                    format!("unknown depth GPUTextureView handle {}", attachment.view)
                })?;
            Ok::<wgpu::RenderPassDepthStencilAttachment<'_>, String>(
                wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load_op(
                            &attachment.depth_load_op,
                            attachment.depth_clear_value,
                        ),
                        store: store_op(&attachment.depth_store_op),
                    }),
                    stencil_ops: None,
                },
            )
        })
        .transpose()?;
    let occlusion_query_set = state
        .occlusion_query_set
        .map(|id| {
            resources
                .query_sets
                .get(&id)
                .ok_or_else(|| format!("unknown occlusion GPUQuerySet handle {id}"))
        })
        .transpose()?;
    let timestamp_writes = state
        .timestamp_writes
        .map(|writes| {
            let query_set = resources.query_sets.get(&writes.query_set).ok_or_else(|| {
                format!("unknown timestamp GPUQuerySet handle {}", writes.query_set)
            })?;
            Ok::<wgpu::RenderPassTimestampWrites<'_>, String>(wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: writes.beginning_of_pass_write_index,
                end_of_pass_write_index: writes.end_of_pass_write_index,
            })
        })
        .transpose()?;
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("hyperthree-js-render-pass"),
        color_attachments: &color_attachments,
        depth_stencil_attachment,
        occlusion_query_set,
        timestamp_writes,
    });
    for command in state.commands {
        match command {
            RenderCommand::SetPipeline(id) => {
                let pipeline = resources
                    .render_pipelines
                    .get(&id)
                    .ok_or_else(|| format!("unknown GPURenderPipeline handle {id}"))?;
                pass.set_pipeline(pipeline);
            }
            RenderCommand::SetBindGroup {
                index,
                bind_group,
                dynamic_offsets,
            } => {
                let bind_group = resources
                    .bind_groups
                    .get(&bind_group)
                    .ok_or_else(|| format!("unknown GPUBindGroup handle {bind_group}"))?;
                pass.set_bind_group(index, bind_group, &dynamic_offsets);
            }
            RenderCommand::SetVertexBuffer {
                slot,
                buffer,
                offset,
                size,
            } => {
                let buffer = resources
                    .buffers
                    .get(&buffer)
                    .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
                let slice = size
                    .map(|size| buffer.slice(offset..offset + size))
                    .unwrap_or_else(|| buffer.slice(offset..));
                pass.set_vertex_buffer(slot, slice);
            }
            RenderCommand::SetIndexBuffer {
                buffer,
                offset,
                format,
            } => {
                let buffer = resources
                    .buffers
                    .get(&buffer)
                    .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
                pass.set_index_buffer(buffer.slice(offset..), index_format(&format));
            }
            RenderCommand::SetViewport {
                x,
                y,
                width,
                height,
                min_depth,
                max_depth,
            } => pass.set_viewport(x, y, width, height, min_depth, max_depth),
            RenderCommand::SetScissorRect {
                x,
                y,
                width,
                height,
            } => pass.set_scissor_rect(x, y, width, height),
            RenderCommand::SetBlendConstant(value) => {
                pass.set_blend_constant(wgpu::Color {
                    r: value[0],
                    g: value[1],
                    b: value[2],
                    a: value[3],
                });
            }
            RenderCommand::SetStencilReference(value) => pass.set_stencil_reference(value),
            RenderCommand::Draw {
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            } => pass.draw(
                first_vertex..first_vertex + vertex_count,
                first_instance..first_instance + instance_count,
            ),
            RenderCommand::DrawIndexed {
                index_count,
                instance_count,
                first_index,
                base_vertex,
                first_instance,
            } => pass.draw_indexed(
                first_index..first_index + index_count,
                base_vertex,
                first_instance..first_instance + instance_count,
            ),
            RenderCommand::DrawIndirect { buffer, offset } => {
                let buffer = resources
                    .buffers
                    .get(&buffer)
                    .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
                pass.draw_indirect(buffer, offset);
            }
            RenderCommand::DrawIndexedIndirect { buffer, offset } => {
                let buffer = resources
                    .buffers
                    .get(&buffer)
                    .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
                pass.draw_indexed_indirect(buffer, offset);
            }
            RenderCommand::BeginOcclusionQuery(index) => pass.begin_occlusion_query(index),
            RenderCommand::EndOcclusionQuery => pass.end_occlusion_query(),
            RenderCommand::ExecuteBundles(ids) => {
                let bundles = ids
                    .iter()
                    .map(|id| {
                        resources
                            .render_bundles
                            .get(id)
                            .ok_or_else(|| format!("unknown GPURenderBundle handle {id}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                pass.execute_bundles(bundles);
            }
        }
    }
    drop(pass);
    Ok(())
}

fn encode_render_bundle_command<'a>(
    encoder: &mut wgpu::RenderBundleEncoder<'a>,
    resources: &'a Resources,
    command: RenderCommand,
) -> Result<(), String> {
    match command {
        RenderCommand::SetPipeline(id) => {
            let pipeline = resources
                .render_pipelines
                .get(&id)
                .ok_or_else(|| format!("unknown GPURenderPipeline handle {id}"))?;
            encoder.set_pipeline(pipeline);
        }
        RenderCommand::SetBindGroup {
            index,
            bind_group,
            dynamic_offsets,
        } => {
            let bind_group = resources
                .bind_groups
                .get(&bind_group)
                .ok_or_else(|| format!("unknown GPUBindGroup handle {bind_group}"))?;
            encoder.set_bind_group(index, bind_group, &dynamic_offsets);
        }
        RenderCommand::SetVertexBuffer {
            slot,
            buffer,
            offset,
            size,
        } => {
            let buffer = resources
                .buffers
                .get(&buffer)
                .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
            let slice = size
                .map(|size| buffer.slice(offset..offset + size))
                .unwrap_or_else(|| buffer.slice(offset..));
            encoder.set_vertex_buffer(slot, slice);
        }
        RenderCommand::SetIndexBuffer {
            buffer,
            offset,
            format,
        } => {
            let buffer = resources
                .buffers
                .get(&buffer)
                .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
            encoder.set_index_buffer(buffer.slice(offset..), index_format(&format));
        }
        RenderCommand::Draw {
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        } => encoder.draw(
            first_vertex..first_vertex + vertex_count,
            first_instance..first_instance + instance_count,
        ),
        RenderCommand::DrawIndexed {
            index_count,
            instance_count,
            first_index,
            base_vertex,
            first_instance,
        } => encoder.draw_indexed(
            first_index..first_index + index_count,
            base_vertex,
            first_instance..first_instance + instance_count,
        ),
        RenderCommand::DrawIndirect { .. } | RenderCommand::DrawIndexedIndirect { .. } => {
            return Err("indirect draws are not supported in render bundles".to_string())
        }
        RenderCommand::SetViewport { .. }
        | RenderCommand::SetScissorRect { .. }
        | RenderCommand::SetBlendConstant(_)
        | RenderCommand::SetStencilReference(_)
        | RenderCommand::BeginOcclusionQuery(_)
        | RenderCommand::EndOcclusionQuery
        | RenderCommand::ExecuteBundles(_) => {
            return Err("unsupported command in render bundle".to_string())
        }
    }
    Ok(())
}

fn encode_compute_pass(
    encoder: &mut wgpu::CommandEncoder,
    resources: &Resources,
    state: ComputePassState,
) -> Result<(), String> {
    let timestamp_writes = state
        .timestamp_writes
        .map(|writes| {
            let query_set = resources.query_sets.get(&writes.query_set).ok_or_else(|| {
                format!("unknown timestamp GPUQuerySet handle {}", writes.query_set)
            })?;
            Ok::<wgpu::ComputePassTimestampWrites<'_>, String>(wgpu::ComputePassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: writes.beginning_of_pass_write_index,
                end_of_pass_write_index: writes.end_of_pass_write_index,
            })
        })
        .transpose()?;
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("hyperthree-js-compute-pass"),
        timestamp_writes,
    });
    for command in state.commands {
        match command {
            ComputeCommand::SetPipeline(id) => {
                let pipeline = resources
                    .compute_pipelines
                    .get(&id)
                    .ok_or_else(|| format!("unknown GPUComputePipeline handle {id}"))?;
                pass.set_pipeline(pipeline);
            }
            ComputeCommand::SetBindGroup {
                index,
                bind_group,
                dynamic_offsets,
            } => {
                let bind_group = resources
                    .bind_groups
                    .get(&bind_group)
                    .ok_or_else(|| format!("unknown GPUBindGroup handle {bind_group}"))?;
                pass.set_bind_group(index, bind_group, &dynamic_offsets);
            }
            ComputeCommand::DispatchWorkgroups { x, y, z } => pass.dispatch_workgroups(x, y, z),
            ComputeCommand::DispatchWorkgroupsIndirect { buffer, offset } => {
                let buffer = resources
                    .buffers
                    .get(&buffer)
                    .ok_or_else(|| format!("unknown GPUBuffer handle {buffer}"))?;
                pass.dispatch_workgroups_indirect(buffer, offset);
            }
        }
    }
    drop(pass);
    Ok(())
}

#[derive(Debug)]
struct OwnedVertexBufferLayout {
    array_stride: wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode,
    attributes: Vec<wgpu::VertexAttribute>,
}

fn multisample_state(value: &Value) -> Result<wgpu::MultisampleState, String> {
    let count = json_u32(value, "count", 1);
    if count == 0 {
        return Err("GPURenderPipeline.multisample.count must be greater than zero".to_string());
    }
    Ok(wgpu::MultisampleState {
        count,
        mask: json_u32(value, "mask", !0) as u64,
        alpha_to_coverage_enabled: json_bool(value, "alphaToCoverageEnabled", false),
    })
}

fn bind_group_layout_entry(value: &Value) -> Result<wgpu::BindGroupLayoutEntry, String> {
    let binding = json_u32(value, "binding", 0);
    let visibility = shader_stages(json_u32(value, "visibility", 0));
    let ty = if let Some(buffer) = value.get("buffer") {
        wgpu::BindingType::Buffer {
            ty: match json_string(buffer, "type").as_deref().unwrap_or("uniform") {
                "storage" => wgpu::BufferBindingType::Storage { read_only: false },
                "read-only-storage" => wgpu::BufferBindingType::Storage { read_only: true },
                _ => wgpu::BufferBindingType::Uniform,
            },
            has_dynamic_offset: json_bool(buffer, "hasDynamicOffset", false),
            min_binding_size: non_zero_u64(json_u64_or(buffer, "minBindingSize", 0)),
        }
    } else if let Some(sampler) = value.get("sampler") {
        wgpu::BindingType::Sampler(
            match json_string(sampler, "type")
                .as_deref()
                .unwrap_or("filtering")
            {
                "non-filtering" => wgpu::SamplerBindingType::NonFiltering,
                "comparison" => wgpu::SamplerBindingType::Comparison,
                _ => wgpu::SamplerBindingType::Filtering,
            },
        )
    } else if let Some(texture) = value.get("texture") {
        wgpu::BindingType::Texture {
            sample_type: match json_string(texture, "sampleType")
                .as_deref()
                .unwrap_or("float")
            {
                "unfilterable-float" => wgpu::TextureSampleType::Float { filterable: false },
                "depth" => wgpu::TextureSampleType::Depth,
                "sint" => wgpu::TextureSampleType::Sint,
                "uint" => wgpu::TextureSampleType::Uint,
                _ => wgpu::TextureSampleType::Float { filterable: true },
            },
            view_dimension: texture_view_dimension(json_string(texture, "viewDimension")),
            multisampled: json_bool(texture, "multisampled", false),
        }
    } else if let Some(storage_texture) = value.get("storageTexture") {
        wgpu::BindingType::StorageTexture {
            access: match json_string(storage_texture, "access")
                .as_deref()
                .unwrap_or("write-only")
            {
                "read-only" => wgpu::StorageTextureAccess::ReadOnly,
                "read-write" => wgpu::StorageTextureAccess::ReadWrite,
                _ => wgpu::StorageTextureAccess::WriteOnly,
            },
            format: texture_format(
                json_string(storage_texture, "format")
                    .as_deref()
                    .unwrap_or("rgba8unorm"),
            ),
            view_dimension: texture_view_dimension(json_string(storage_texture, "viewDimension")),
        }
    } else if value.get("externalTexture").is_some() {
        // wgpu 0.20 has no separate external-texture binding type. The
        // native bridge imports RGBA frames into a regular 2D texture, while
        // the shader sanitizer below maps the WebGPU external texture syntax
        // to the equivalent native texture sampling operations.
        wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        }
    } else {
        return Err(format!("unsupported bind group layout entry {binding}"));
    };
    Ok(wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    })
}

fn binding_resource<'a>(
    value: &Value,
    resources: &'a Resources,
) -> Result<wgpu::BindingResource<'a>, String> {
    match json_string(value, "kind").as_deref() {
        Some("buffer") => {
            let id = json_u64(value, "buffer")?;
            let buffer = resources
                .buffers
                .get(&id)
                .ok_or_else(|| format!("unknown GPUBuffer handle {id}"))?;
            Ok(wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: json_u64_or(value, "offset", 0),
                size: non_zero_u64(json_u64_or(value, "size", 0)),
            }))
        }
        Some("textureView") => {
            let id = json_u64(value, "view")?;
            let view = resources
                .texture_views
                .get(&id)
                .ok_or_else(|| format!("unknown GPUTextureView handle {id}"))?;
            Ok(wgpu::BindingResource::TextureView(view))
        }
        Some("sampler") => {
            let id = json_u64(value, "sampler")?;
            let sampler = resources
                .samplers
                .get(&id)
                .ok_or_else(|| format!("unknown GPUSampler handle {id}"))?;
            Ok(wgpu::BindingResource::Sampler(sampler))
        }
        _ => Err("unsupported GPUBindGroup resource".to_string()),
    }
}

fn vertex_buffer_layout_owned(value: &Value) -> Result<OwnedVertexBufferLayout, String> {
    let attributes = value
        .get("attributes")
        .and_then(Value::as_array)
        .ok_or_else(|| "GPUVertexBufferLayout.attributes must be an array".to_string())?
        .iter()
        .map(|attribute| {
            Ok(wgpu::VertexAttribute {
                format: vertex_format(
                    json_string(attribute, "format")
                        .as_deref()
                        .unwrap_or("float32x4"),
                ),
                offset: json_u64_or(attribute, "offset", 0),
                shader_location: json_u32(attribute, "shaderLocation", 0),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(OwnedVertexBufferLayout {
        array_stride: json_u64_or(value, "arrayStride", 0),
        step_mode: match json_string(value, "stepMode")
            .as_deref()
            .unwrap_or("vertex")
        {
            "instance" => wgpu::VertexStepMode::Instance,
            _ => wgpu::VertexStepMode::Vertex,
        },
        attributes,
    })
}

fn color_target_state(value: &Value) -> Result<Option<wgpu::ColorTargetState>, String> {
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(wgpu::ColorTargetState {
        format: texture_format(
            json_string(value, "format")
                .as_deref()
                .unwrap_or("bgra8unorm-srgb"),
        ),
        blend: value.get("blend").map(blend_state).transpose()?,
        write_mask: wgpu::ColorWrites::from_bits_truncate(
            json_u64_or(value, "writeMask", 15) as u32
        ),
    }))
}

fn blend_state(value: &Value) -> Result<wgpu::BlendState, String> {
    Ok(wgpu::BlendState {
        color: blend_component(value.get("color").unwrap_or(value))?,
        alpha: blend_component(value.get("alpha").unwrap_or(value))?,
    })
}

fn blend_component(value: &Value) -> Result<wgpu::BlendComponent, String> {
    Ok(wgpu::BlendComponent {
        src_factor: blend_factor(json_string(value, "srcFactor").as_deref().unwrap_or("one")),
        dst_factor: blend_factor(json_string(value, "dstFactor").as_deref().unwrap_or("zero")),
        operation: blend_operation(json_string(value, "operation").as_deref().unwrap_or("add")),
    })
}

fn depth_stencil_state(value: &Value) -> Result<wgpu::DepthStencilState, String> {
    Ok(wgpu::DepthStencilState {
        format: texture_format(
            json_string(value, "format")
                .as_deref()
                .unwrap_or("depth24plus"),
        ),
        depth_write_enabled: json_bool(value, "depthWriteEnabled", true),
        depth_compare: compare_function(json_string(value, "depthCompare")),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    })
}

fn primitive_state(value: Option<&Value>) -> wgpu::PrimitiveState {
    let value = value.unwrap_or(&Value::Null);
    wgpu::PrimitiveState {
        topology: match json_string(value, "topology")
            .as_deref()
            .unwrap_or("triangle-list")
        {
            "point-list" => wgpu::PrimitiveTopology::PointList,
            "line-list" => wgpu::PrimitiveTopology::LineList,
            "line-strip" => wgpu::PrimitiveTopology::LineStrip,
            "triangle-strip" => wgpu::PrimitiveTopology::TriangleStrip,
            "triangle-fan" => wgpu::PrimitiveTopology::TriangleList,
            _ => wgpu::PrimitiveTopology::TriangleList,
        },
        strip_index_format: json_string(value, "stripIndexFormat")
            .map(|format| index_format(&format)),
        front_face: match json_string(value, "frontFace").as_deref().unwrap_or("ccw") {
            "cw" => wgpu::FrontFace::Cw,
            _ => wgpu::FrontFace::Ccw,
        },
        cull_mode: match json_string(value, "cullMode").as_deref() {
            Some("front") => Some(wgpu::Face::Front),
            Some("back") => Some(wgpu::Face::Back),
            _ => None,
        },
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
    }
}

fn color_attachment_state(value: &Value) -> Result<ColorAttachmentState, String> {
    Ok(ColorAttachmentState {
        view: json_u64(value, "view")?,
        resolve_target: value.get("resolveTarget").and_then(Value::as_u64),
        load_op: json_string(value, "loadOp").unwrap_or_else(|| "load".to_string()),
        store_op: json_string(value, "storeOp").unwrap_or_else(|| "store".to_string()),
        clear_value: json_color(value.get("clearValue")),
    })
}

fn depth_stencil_attachment_state(value: &Value) -> Result<DepthStencilAttachmentState, String> {
    Ok(DepthStencilAttachmentState {
        view: json_u64(value, "view")?,
        depth_load_op: json_string(value, "depthLoadOp").unwrap_or_else(|| "load".to_string()),
        depth_store_op: json_string(value, "depthStoreOp").unwrap_or_else(|| "store".to_string()),
        depth_clear_value: json_f32(value, "depthClearValue", 1.0),
    })
}

fn color_load_op(operation: &str, clear: [f64; 4]) -> wgpu::LoadOp<wgpu::Color> {
    match operation {
        "clear" => wgpu::LoadOp::Clear(wgpu::Color {
            r: clear[0],
            g: clear[1],
            b: clear[2],
            a: clear[3],
        }),
        _ => wgpu::LoadOp::Load,
    }
}

fn depth_load_op(operation: &str, clear: f32) -> wgpu::LoadOp<f32> {
    match operation {
        "clear" => wgpu::LoadOp::Clear(clear),
        _ => wgpu::LoadOp::Load,
    }
}

fn store_op(operation: &str) -> wgpu::StoreOp {
    match operation {
        "discard" => wgpu::StoreOp::Discard,
        _ => wgpu::StoreOp::Store,
    }
}

fn shader_stages(bits: u32) -> wgpu::ShaderStages {
    let mut stages = wgpu::ShaderStages::empty();
    if bits & 1 != 0 {
        stages |= wgpu::ShaderStages::VERTEX;
    }
    if bits & 2 != 0 {
        stages |= wgpu::ShaderStages::FRAGMENT;
    }
    if bits & 4 != 0 {
        stages |= wgpu::ShaderStages::COMPUTE;
    }
    stages
}

fn texture_view_dimension(value: Option<String>) -> wgpu::TextureViewDimension {
    match value.as_deref().unwrap_or("2d") {
        "1d" => wgpu::TextureViewDimension::D1,
        "2d-array" => wgpu::TextureViewDimension::D2Array,
        "cube" => wgpu::TextureViewDimension::Cube,
        "cube-array" => wgpu::TextureViewDimension::CubeArray,
        "3d" => wgpu::TextureViewDimension::D3,
        _ => wgpu::TextureViewDimension::D2,
    }
}

fn texture_dimension(value: &str) -> wgpu::TextureDimension {
    match value {
        "1d" => wgpu::TextureDimension::D1,
        "3d" => wgpu::TextureDimension::D3,
        _ => wgpu::TextureDimension::D2,
    }
}

fn texture_view_descriptor(value: &Value) -> wgpu::TextureViewDescriptor<'static> {
    wgpu::TextureViewDescriptor {
        label: None,
        format: json_string(value, "format").map(|format| texture_format(&format)),
        dimension: json_string(value, "dimension")
            .map(|dimension| texture_view_dimension(Some(dimension))),
        aspect: match json_string(value, "aspect").as_deref() {
            Some("depth-only") => wgpu::TextureAspect::DepthOnly,
            Some("stencil-only") => wgpu::TextureAspect::StencilOnly,
            _ => wgpu::TextureAspect::All,
        },
        base_mip_level: json_u32(value, "baseMipLevel", 0),
        mip_level_count: optional_u32(json_u32(value, "mipLevelCount", 0)),
        base_array_layer: json_u32(value, "baseArrayLayer", 0),
        array_layer_count: optional_u32(json_u32(value, "arrayLayerCount", 0)),
    }
}

fn vertex_format(value: &str) -> wgpu::VertexFormat {
    match value {
        "float32" => wgpu::VertexFormat::Float32,
        "float32x2" => wgpu::VertexFormat::Float32x2,
        "float32x3" => wgpu::VertexFormat::Float32x3,
        "uint32" => wgpu::VertexFormat::Uint32,
        "uint32x2" => wgpu::VertexFormat::Uint32x2,
        "uint32x3" => wgpu::VertexFormat::Uint32x3,
        "uint32x4" => wgpu::VertexFormat::Uint32x4,
        "sint32" => wgpu::VertexFormat::Sint32,
        "sint32x2" => wgpu::VertexFormat::Sint32x2,
        "sint32x3" => wgpu::VertexFormat::Sint32x3,
        "sint32x4" => wgpu::VertexFormat::Sint32x4,
        "unorm8x4" => wgpu::VertexFormat::Unorm8x4,
        "snorm8x4" => wgpu::VertexFormat::Snorm8x4,
        "uint8x4" => wgpu::VertexFormat::Uint8x4,
        "sint8x4" => wgpu::VertexFormat::Sint8x4,
        "unorm16x2" => wgpu::VertexFormat::Unorm16x2,
        "unorm16x4" => wgpu::VertexFormat::Unorm16x4,
        "snorm16x2" => wgpu::VertexFormat::Snorm16x2,
        "snorm16x4" => wgpu::VertexFormat::Snorm16x4,
        "uint16x2" => wgpu::VertexFormat::Uint16x2,
        "uint16x4" => wgpu::VertexFormat::Uint16x4,
        "sint16x2" => wgpu::VertexFormat::Sint16x2,
        "sint16x4" => wgpu::VertexFormat::Sint16x4,
        _ => wgpu::VertexFormat::Float32x4,
    }
}

fn index_format(value: &str) -> wgpu::IndexFormat {
    match value {
        "uint16" => wgpu::IndexFormat::Uint16,
        _ => wgpu::IndexFormat::Uint32,
    }
}

fn address_mode(value: Option<String>) -> wgpu::AddressMode {
    match value.as_deref().unwrap_or("clamp-to-edge") {
        "repeat" => wgpu::AddressMode::Repeat,
        "mirror-repeat" => wgpu::AddressMode::MirrorRepeat,
        _ => wgpu::AddressMode::ClampToEdge,
    }
}

fn filter_mode(value: Option<String>) -> wgpu::FilterMode {
    match value.as_deref().unwrap_or("nearest") {
        "linear" => wgpu::FilterMode::Linear,
        _ => wgpu::FilterMode::Nearest,
    }
}

fn compare_function(value: Option<String>) -> wgpu::CompareFunction {
    match value.as_deref() {
        Some("never") => wgpu::CompareFunction::Never,
        Some("less") => wgpu::CompareFunction::Less,
        Some("equal") => wgpu::CompareFunction::Equal,
        Some("less-equal") => wgpu::CompareFunction::LessEqual,
        Some("greater") => wgpu::CompareFunction::Greater,
        Some("not-equal") => wgpu::CompareFunction::NotEqual,
        Some("greater-equal") => wgpu::CompareFunction::GreaterEqual,
        Some("always") => wgpu::CompareFunction::Always,
        _ => wgpu::CompareFunction::Always,
    }
}

fn blend_factor(value: &str) -> wgpu::BlendFactor {
    match value {
        "zero" => wgpu::BlendFactor::Zero,
        "src" => wgpu::BlendFactor::Src,
        "one-minus-src" => wgpu::BlendFactor::OneMinusSrc,
        "src-alpha" => wgpu::BlendFactor::SrcAlpha,
        "one-minus-src-alpha" => wgpu::BlendFactor::OneMinusSrcAlpha,
        "dst" => wgpu::BlendFactor::Dst,
        "one-minus-dst" => wgpu::BlendFactor::OneMinusDst,
        "dst-alpha" => wgpu::BlendFactor::DstAlpha,
        "one-minus-dst-alpha" => wgpu::BlendFactor::OneMinusDstAlpha,
        "src-alpha-saturated" => wgpu::BlendFactor::SrcAlphaSaturated,
        "constant" => wgpu::BlendFactor::Constant,
        "one-minus-constant" => wgpu::BlendFactor::OneMinusConstant,
        _ => wgpu::BlendFactor::One,
    }
}

fn blend_operation(value: &str) -> wgpu::BlendOperation {
    match value {
        "subtract" => wgpu::BlendOperation::Subtract,
        "reverse-subtract" => wgpu::BlendOperation::ReverseSubtract,
        "min" => wgpu::BlendOperation::Min,
        "max" => wgpu::BlendOperation::Max,
        _ => wgpu::BlendOperation::Add,
    }
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn json_bool(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn json_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("WebGPU descriptor field {key} must be an integer"))
}

fn json_u64_or(value: &Value, key: &str, default: u64) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn json_u32(value: &Value, key: &str, default: u32) -> u32 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(default)
}

fn json_u16(value: &Value, key: &str, default: u16) -> u16 {
    json_u32(value, key, default as u32) as u16
}

fn json_f32(value: &Value, key: &str, default: f32) -> f32 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(default)
}

fn json_color(value: Option<&Value>) -> [f64; 4] {
    let value = value.and_then(Value::as_object);
    [
        value
            .and_then(|value| value.get("r"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        value
            .and_then(|value| value.get("g"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        value
            .and_then(|value| value.get("b"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        value
            .and_then(|value| value.get("a"))
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
    ]
}

fn non_zero_u64(value: u64) -> Option<std::num::NonZeroU64> {
    std::num::NonZeroU64::new(value)
}

fn optional_u32(value: u32) -> Option<u32> {
    (value != 0).then_some(value)
}

fn value_u64(values: &[Value], index: usize) -> Result<u64, String> {
    values
        .get(index)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("WebGPU pass argument {index} must be an integer"))
}

fn value_u64_or(values: &[Value], index: usize, default: u64) -> Result<u64, String> {
    Ok(values.get(index).and_then(Value::as_u64).unwrap_or(default))
}

fn value_u32(values: &[Value], index: usize) -> Result<u32, String> {
    Ok(value_u64(values, index)? as u32)
}

fn value_u32_or(values: &[Value], index: usize, default: u32) -> Result<u32, String> {
    Ok(values
        .get(index)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(default))
}

fn value_i32_or(values: &[Value], index: usize, default: i32) -> Result<i32, String> {
    Ok(values
        .get(index)
        .and_then(Value::as_i64)
        .map(|value| value as i32)
        .unwrap_or(default))
}

fn value_f64(values: &[Value], index: usize) -> Result<f64, String> {
    values
        .get(index)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("WebGPU pass argument {index} must be numeric"))
}

fn value_f32(values: &[Value], index: usize) -> Result<f32, String> {
    Ok(value_f64(values, index)? as f32)
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

fn optional_number_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<Option<f64>> {
    if args.get(index).is_none_or(JsValue::is_undefined) {
        return Ok(None);
    }
    number_arg(args, index, context).map(Some)
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

fn optional_string_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<Option<String>> {
    if args.get(index).is_none_or(JsValue::is_undefined) {
        return Ok(None);
    }
    string_arg(args, index, context).map(Some)
}

fn optional_u32_array_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<Option<[u32; 3]>> {
    let Some(value) = args.get(index) else {
        return Ok(None);
    };
    if value.is_undefined() {
        return Ok(None);
    }
    let source = value.to_string(context)?.to_std_string_escaped();
    let values = serde_json::from_str::<Vec<u32>>(&source).map_err(|_| {
        JsNativeError::typ().with_message("WebGPU origin must be a JSON uint array")
    })?;
    if values.len() > 3 {
        return Err(JsNativeError::range()
            .with_message("WebGPU origin has too many components")
            .into());
    }
    Ok(Some([
        values.first().copied().unwrap_or(0),
        values.get(1).copied().unwrap_or(0),
        values.get(2).copied().unwrap_or(0),
    ]))
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

fn sanitize_wgsl(source: &str) -> String {
    let source = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("diagnostic("))
        .collect::<Vec<_>>()
        .join("\n");
    let source = normalize_texture_load_levels(&source);
    let source = source
        .replace("texture_external", "texture_2d<f32>")
        .replace("textureSampleBaseClampToEdge", "textureSample");
    normalize_abstract_integer_casts(&source)
}

fn normalize_abstract_integer_casts(source: &str) -> String {
    let source = normalize_integer_cast(source, "u32(", "u");
    normalize_integer_cast(&source, "i32(", "i")
}

fn normalize_integer_cast(source: &str, function: &str, suffix: &str) -> String {
    let mut normalized = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find(function) {
        let start = cursor + relative_start;
        normalized.push_str(&source[cursor..start]);
        let open = start + function.len() - 1;
        let Some(end) = matching_parenthesis(source, open) else {
            normalized.push_str(&source[start..]);
            return normalized;
        };
        let value = source[open + 1..end].trim();
        if value.contains('.') || value.contains('e') || value.contains('E') {
            if let Ok(value) = value.parse::<f64>() {
                if value.is_finite() && value.fract() == 0.0 {
                    normalized.push_str(&format!("{}{}", value as i64, suffix));
                    cursor = end + 1;
                    continue;
                }
            }
        }
        normalized.push_str(&source[start..=end]);
        cursor = end + 1;
    }
    normalized.push_str(&source[cursor..]);
    normalized
}

fn normalize_texture_load_levels(source: &str) -> String {
    const FUNCTION: &str = "textureLoad(";
    let mut normalized = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find(FUNCTION) {
        let start = cursor + relative_start;
        normalized.push_str(&source[cursor..start]);
        let open = start + FUNCTION.len() - 1;
        let Some(end) = matching_parenthesis(source, open) else {
            normalized.push_str(&source[start..]);
            return normalized;
        };
        normalized.push_str(&normalize_texture_load_call(&source[start..=end]));
        cursor = end + 1;
    }
    normalized.push_str(&source[cursor..]);
    normalized
}

fn matching_parenthesis(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (index, character) in source.char_indices().skip_while(|(index, _)| *index < open) {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_texture_load_call(call: &str) -> String {
    let open = call.find('(').unwrap_or(0);
    let close = call.len().saturating_sub(1);
    let mut depth = 0;
    let mut last_top_level_comma = None;
    for (index, character) in call.char_indices().skip_while(|(index, _)| *index <= open) {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => last_top_level_comma = Some(index),
            _ => {}
        }
    }
    let Some(comma) = last_top_level_comma else {
        return call.to_string();
    };
    let suffix = &call[comma + 1..close];
    let trimmed = suffix.trim_start();
    let Some(level) = trimmed.strip_prefix("u32(") else {
        return call.to_string();
    };
    let level = level.trim();
    let level = level.strip_suffix(')').unwrap_or(level).trim();
    let level = level.strip_suffix('u').unwrap_or(level).trim();
    let indentation = &suffix[..suffix.len() - trimmed.len()];
    let replacement = format!("{indentation}i32({level})");
    format!("{}{}{}", &call[..comma + 1], replacement, &call[close..])
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
    if bits & 256 != 0 {
        usage |= wgpu::BufferUsages::INDIRECT;
    }
    if bits & 512 != 0 {
        usage |= wgpu::BufferUsages::QUERY_RESOLVE;
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

fn texture_usage_for_dimension(bits: u64, dimension: &str) -> wgpu::TextureUsages {
    let mut usage = texture_usage(bits);
    if matches!(
        texture_dimension(dimension),
        wgpu::TextureDimension::D1 | wgpu::TextureDimension::D3
    ) {
        // wgpu rejects RENDER_ATTACHMENT for 1D/3D textures even when a
        // higher-level renderer includes the usage in its shared descriptor.
        // Keep the valid usages so Three.js Data3DTexture/DataTexture paths
        // remain portable across native backends.
        usage.remove(wgpu::TextureUsages::RENDER_ATTACHMENT);
    }
    usage
}

fn texture_format(format: &str) -> wgpu::TextureFormat {
    match format {
        "r8unorm" => wgpu::TextureFormat::R8Unorm,
        "r8snorm" => wgpu::TextureFormat::R8Snorm,
        "r8uint" => wgpu::TextureFormat::R8Uint,
        "r8sint" => wgpu::TextureFormat::R8Sint,
        "r16uint" => wgpu::TextureFormat::R16Uint,
        "r16sint" => wgpu::TextureFormat::R16Sint,
        "r16float" => wgpu::TextureFormat::R16Float,
        "rg8unorm" => wgpu::TextureFormat::Rg8Unorm,
        "rg8snorm" => wgpu::TextureFormat::Rg8Snorm,
        "rg8uint" => wgpu::TextureFormat::Rg8Uint,
        "rg8sint" => wgpu::TextureFormat::Rg8Sint,
        "rg16uint" => wgpu::TextureFormat::Rg16Uint,
        "rg16sint" => wgpu::TextureFormat::Rg16Sint,
        "rg16float" => wgpu::TextureFormat::Rg16Float,
        "rgba8unorm" => wgpu::TextureFormat::Rgba8Unorm,
        "bgra8unorm" => wgpu::TextureFormat::Bgra8Unorm,
        "bgra8unorm-srgb" => wgpu::TextureFormat::Bgra8UnormSrgb,
        "rgba8unorm-srgb" => wgpu::TextureFormat::Rgba8UnormSrgb,
        "rgba8snorm" => wgpu::TextureFormat::Rgba8Snorm,
        "rgba8uint" => wgpu::TextureFormat::Rgba8Uint,
        "rgba8sint" => wgpu::TextureFormat::Rgba8Sint,
        "rgba16uint" => wgpu::TextureFormat::Rgba16Uint,
        "rgba16sint" => wgpu::TextureFormat::Rgba16Sint,
        "rgba16unorm" => wgpu::TextureFormat::Rgba16Unorm,
        "rgba16snorm" => wgpu::TextureFormat::Rgba16Snorm,
        "rgba16float" => wgpu::TextureFormat::Rgba16Float,
        "rg32uint" => wgpu::TextureFormat::Rg32Uint,
        "rg32sint" => wgpu::TextureFormat::Rg32Sint,
        "rg32float" => wgpu::TextureFormat::Rg32Float,
        "rgba32uint" => wgpu::TextureFormat::Rgba32Uint,
        "rgba32sint" => wgpu::TextureFormat::Rgba32Sint,
        "rgba32float" => wgpu::TextureFormat::Rgba32Float,
        "rgb9e5ufloat" => wgpu::TextureFormat::Rgb9e5Ufloat,
        "rgb10a2uint" => wgpu::TextureFormat::Rgb10a2Uint,
        "rgb10a2unorm" => wgpu::TextureFormat::Rgb10a2Unorm,
        "rg11b10ufloat" => wgpu::TextureFormat::Rg11b10Float,
        "r32float" => wgpu::TextureFormat::R32Float,
        "r32uint" => wgpu::TextureFormat::R32Uint,
        "r32sint" => wgpu::TextureFormat::R32Sint,
        "depth24plus" => wgpu::TextureFormat::Depth24Plus,
        "depth24plus-stencil8" => wgpu::TextureFormat::Depth24PlusStencil8,
        "depth32float" => wgpu::TextureFormat::Depth32Float,
        "depth32float-stencil8" => wgpu::TextureFormat::Depth32FloatStencil8,
        "depth16unorm" => wgpu::TextureFormat::Depth16Unorm,
        "stencil8" => wgpu::TextureFormat::Stencil8,
        "bc1-rgba-unorm" => wgpu::TextureFormat::Bc1RgbaUnorm,
        "bc1-rgba-unorm-srgb" => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        "bc2-rgba-unorm" => wgpu::TextureFormat::Bc2RgbaUnorm,
        "bc2-rgba-unorm-srgb" => wgpu::TextureFormat::Bc2RgbaUnormSrgb,
        "bc3-rgba-unorm" => wgpu::TextureFormat::Bc3RgbaUnorm,
        "bc3-rgba-unorm-srgb" => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
        "bc4-r-unorm" => wgpu::TextureFormat::Bc4RUnorm,
        "bc4-r-snorm" => wgpu::TextureFormat::Bc4RSnorm,
        "bc5-rg-unorm" => wgpu::TextureFormat::Bc5RgUnorm,
        "bc5-rg-snorm" => wgpu::TextureFormat::Bc5RgSnorm,
        "bc6h-rgb-ufloat" => wgpu::TextureFormat::Bc6hRgbUfloat,
        "bc6h-rgb-float" => wgpu::TextureFormat::Bc6hRgbFloat,
        "bc7-rgba-unorm" => wgpu::TextureFormat::Bc7RgbaUnorm,
        "bc7-rgba-unorm-srgb" => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
        "etc2-rgb8unorm" => wgpu::TextureFormat::Etc2Rgb8Unorm,
        "etc2-rgb8unorm-srgb" => wgpu::TextureFormat::Etc2Rgb8UnormSrgb,
        "etc2-rgb8a1unorm" => wgpu::TextureFormat::Etc2Rgb8A1Unorm,
        "etc2-rgb8a1unorm-srgb" => wgpu::TextureFormat::Etc2Rgb8A1UnormSrgb,
        "etc2-rgba8unorm" => wgpu::TextureFormat::Etc2Rgba8Unorm,
        "etc2-rgba8unorm-srgb" => wgpu::TextureFormat::Etc2Rgba8UnormSrgb,
        "eac-r11unorm" => wgpu::TextureFormat::EacR11Unorm,
        "eac-r11snorm" => wgpu::TextureFormat::EacR11Snorm,
        "eac-rg11unorm" => wgpu::TextureFormat::EacRg11Unorm,
        "eac-rg11snorm" => wgpu::TextureFormat::EacRg11Snorm,
        format if format.starts_with("astc-") => astc_texture_format(format),
        _ => wgpu::TextureFormat::Rgba8Unorm,
    }
}

fn astc_texture_format(format: &str) -> wgpu::TextureFormat {
    let (block, srgb) = match format {
        "astc-4x4-unorm" => (wgpu::AstcBlock::B4x4, false),
        "astc-4x4-unorm-srgb" => (wgpu::AstcBlock::B4x4, true),
        "astc-5x4-unorm" => (wgpu::AstcBlock::B5x4, false),
        "astc-5x4-unorm-srgb" => (wgpu::AstcBlock::B5x4, true),
        "astc-5x5-unorm" => (wgpu::AstcBlock::B5x5, false),
        "astc-5x5-unorm-srgb" => (wgpu::AstcBlock::B5x5, true),
        "astc-6x5-unorm" => (wgpu::AstcBlock::B6x5, false),
        "astc-6x5-unorm-srgb" => (wgpu::AstcBlock::B6x5, true),
        "astc-6x6-unorm" => (wgpu::AstcBlock::B6x6, false),
        "astc-6x6-unorm-srgb" => (wgpu::AstcBlock::B6x6, true),
        "astc-8x5-unorm" => (wgpu::AstcBlock::B8x5, false),
        "astc-8x5-unorm-srgb" => (wgpu::AstcBlock::B8x5, true),
        "astc-8x6-unorm" => (wgpu::AstcBlock::B8x6, false),
        "astc-8x6-unorm-srgb" => (wgpu::AstcBlock::B8x6, true),
        "astc-8x8-unorm" => (wgpu::AstcBlock::B8x8, false),
        "astc-8x8-unorm-srgb" => (wgpu::AstcBlock::B8x8, true),
        "astc-10x5-unorm" => (wgpu::AstcBlock::B10x5, false),
        "astc-10x5-unorm-srgb" => (wgpu::AstcBlock::B10x5, true),
        "astc-10x6-unorm" => (wgpu::AstcBlock::B10x6, false),
        "astc-10x6-unorm-srgb" => (wgpu::AstcBlock::B10x6, true),
        "astc-10x8-unorm" => (wgpu::AstcBlock::B10x8, false),
        "astc-10x8-unorm-srgb" => (wgpu::AstcBlock::B10x8, true),
        "astc-10x10-unorm" => (wgpu::AstcBlock::B10x10, false),
        "astc-10x10-unorm-srgb" => (wgpu::AstcBlock::B10x10, true),
        "astc-12x10-unorm" => (wgpu::AstcBlock::B12x10, false),
        "astc-12x10-unorm-srgb" => (wgpu::AstcBlock::B12x10, true),
        "astc-12x12-unorm" => (wgpu::AstcBlock::B12x12, false),
        "astc-12x12-unorm-srgb" => (wgpu::AstcBlock::B12x12, true),
        _ => return wgpu::TextureFormat::Rgba8Unorm,
    };
    wgpu::TextureFormat::Astc {
        block,
        channel: if srgb {
            wgpu::AstcChannel::UnormSrgb
        } else {
            wgpu::AstcChannel::Unorm
        },
    }
}

const WEBGPU_BOOTSTRAP: &str = r#"
(() => {
  if (globalThis.navigator?.gpu) return;
  // Three.js' node-cache can transiently use an undefined chain key while
  // nested node graphs are being compiled. Browsers reject that key, but Boa
  // otherwise aborts the whole renderer initialization. Preserve native
  // WeakMap behavior for object keys while treating non-object keys as
  // cache misses in the embedded runtime.
  const NativeWeakMap = globalThis.WeakMap;
  class HyperThreeWeakMap {
    constructor(entries) {
      this.__native = new NativeWeakMap();
      if (entries) for (const entry of entries) this.set(entry[0], entry[1]);
    }
    get(key) { return key == null ? undefined : this.__native.get(key); }
    has(key) { return key == null ? false : this.__native.has(key); }
    set(key, value) {
      if (key != null) this.__native.set(key, value);
      return this;
    }
    delete(key) {
      if (key == null) {
        return false;
      }
      return this.__native.delete(key);
    }
  }
  globalThis.WeakMap = HyperThreeWeakMap;
  const makeHandle = (id, methods = {}) => Object.assign({ __hyperthreeHandle: id }, methods);
  const handleId = value => value?.__hyperthreeHandle ?? value;
  const descriptorJson = value => JSON.stringify(value ?? {});
  const bufferUsage = { MAP_READ: 1, MAP_WRITE: 2, COPY_SRC: 4, COPY_DST: 8, INDEX: 16, VERTEX: 32, UNIFORM: 64, STORAGE: 128, INDIRECT: 256, QUERY_RESOLVE: 512 };
  const textureUsage = { COPY_SRC: 1, COPY_DST: 2, TEXTURE_BINDING: 4, STORAGE_BINDING: 8, RENDER_ATTACHMENT: 16 };
  globalThis.GPUShaderStage = globalThis.GPUShaderStage || { VERTEX: 1, FRAGMENT: 2, COMPUTE: 4 };
  globalThis.GPUMapMode = globalThis.GPUMapMode || { READ: 1, WRITE: 2 };
  globalThis.GPUFeatureName = globalThis.GPUFeatureName || { 'timestamp-query': 'timestamp-query' };
  globalThis.HTMLCanvasElement = globalThis.HTMLCanvasElement || function HTMLCanvasElement() {};
  globalThis.HTMLVideoElement = globalThis.HTMLVideoElement || class HTMLVideoElement {
    constructor() {
      this.__listeners = new Map();
      this.__frameCallbacks = new Map();
      this.__nextFrameCallbackId = 0;
      this.__presentedFrames = 0;
      this.__loadPromise = Promise.resolve();
      this.__loadToken = 0;
      this.__rafId = null;
      this.__frames = [];
      this.__frameIndex = 0;
      this.__lastPumpTime = null;
      this._src = '';
      this.currentFrame = null;
      this.data = new Uint8Array(0);
      this.videoWidth = 0;
      this.videoHeight = 0;
      this._currentTime = 0;
      this.duration = 0;
      this.readyState = 0;
      this.networkState = 0;
      this.paused = true;
      this.ended = false;
      this.loop = false;
      this.autoplay = false;
      this.muted = false;
      this.volume = 1;
      this.playbackRate = 1;
      this.crossOrigin = null;
      this.preload = 'auto';
      this.HAVE_NOTHING = 0;
      this.HAVE_METADATA = 1;
      this.HAVE_CURRENT_DATA = 2;
      this.HAVE_FUTURE_DATA = 3;
      this.HAVE_ENOUGH_DATA = 4;
    }
    get src() { return this._src; }
    set src(value) { this._src = String(value); this.load(); }
    get currentSrc() { return this._src; }
    get width() { return this.videoWidth; }
    get height() { return this.videoHeight; }
    get currentTime() { return this._currentTime; }
    set currentTime(value) {
      let nextTime = Number(value);
      if (!Number.isFinite(nextTime) || nextTime < 0) nextTime = 0;
      if (this.duration > 0) nextTime = Math.min(nextTime, this.duration);
      this._currentTime = nextTime;
      if (this.currentFrame && this.__frames.length > 0) {
        const nextFrame = this.__frameIndexForTime(nextTime);
        if (nextFrame !== this.__frameIndex) this.__setFrame(nextFrame);
      }
    }
    addEventListener(type, listener) {
      if (typeof listener !== 'function') return;
      const listeners = this.__listeners.get(type) || new Set();
      listeners.add(listener);
      this.__listeners.set(type, listeners);
    }
    removeEventListener(type, listener) { this.__listeners.get(type)?.delete(listener); }
    dispatchEvent(event) {
      for (const listener of this.__listeners.get(event.type) || []) listener.call(this, event);
      const handler = this['on' + event.type];
      if (typeof handler === 'function') handler.call(this, event);
      return !event.defaultPrevented;
    }
    setAttribute(name, value) { if (String(name).toLowerCase() === 'src') this.src = value; }
    getAttribute(name) { return String(name).toLowerCase() === 'src' ? this._src : null; }
    canPlayType(type) { return String(type || '').startsWith('image/') ? 'probably' : ''; }
    __setFrame(index) {
      const frame = this.__frames[index];
      if (!frame) return;
      this.currentFrame?.close();
      this.__frameIndex = index;
      this.videoWidth = frame.width;
      this.videoHeight = frame.height;
      this.data = new Uint8Array(frame.data);
      this.currentFrame = new VideoFrame({
        width: frame.width,
        height: frame.height,
        data: this.data,
      }, { timestamp: Math.round(this.currentTime * 1000000) });
    }
    __frameIndexForTime(time) {
      let elapsed = 0;
      for (let index = 0; index < this.__frames.length; index += 1) {
        elapsed += this.__frames[index].durationMs / 1000;
        if (time < elapsed) return index;
      }
      return Math.max(0, this.__frames.length - 1);
    }
    load() {
      const token = ++this.__loadToken;
      this.readyState = this._src ? 1 : 0;
      this.networkState = this._src ? 2 : 0;
      this.ended = false;
      this.__frames = [];
      this.__frameIndex = 0;
      this.__lastPumpTime = null;
      if (!this._src) return;
      this.__loadPromise = fetch(this._src)
        .then(response => response.blob())
        .then(blob => {
          const isAnimatedGif = typeof globalThis.__hyperthreeDecodeAnimatedImage === 'function' &&
            /\.gif(?:[?#].*)?$/i.test(this._src);
          if (isAnimatedGif) {
            const animated = globalThis.__hyperthreeDecodeAnimatedImage(blob);
            return { animated: true, frames: animated.frames };
          }
          return createImageBitmap(blob).then(bitmap => {
            const frame = { width: bitmap.width, height: bitmap.height, data: new Uint8Array(bitmap.data), durationMs: 0 };
            bitmap.close?.();
            return { animated: false, frames: [frame] };
          });
        })
        .then(payload => {
          if (token !== this.__loadToken) return;
          this.__frames = payload.frames;
          this.currentTime = 0;
          this.duration = payload.animated ? payload.frames.reduce((total, frame) => total + frame.durationMs, 0) / 1000 : 0;
          this.__setFrame(0);
          this.readyState = 2;
          this.networkState = 1;
          this.dispatchEvent(new Event('loadedmetadata'));
          this.dispatchEvent(new Event('loadeddata'));
          this.dispatchEvent(new Event('canplay'));
        })
        .catch(error => {
          if (token !== this.__loadToken) return;
          this.readyState = 0;
          this.networkState = 3;
          this.dispatchEvent(Object.assign(new Event('error'), { error }));
        });
    }
    __notifyFrame() {
      if (!this.currentFrame) return;
      this.__presentedFrames += 1;
      const metadata = {
        mediaTime: this.currentTime,
        presentedFrames: this.__presentedFrames,
        expectedDisplayTime: performance.now(),
        width: this.videoWidth,
        height: this.videoHeight,
      };
      const callbacks = [...this.__frameCallbacks.values()];
      this.__frameCallbacks.clear();
      for (const callback of callbacks) callback(performance.now(), metadata);
    }
    __pump() {
      if (this.paused || !this.currentFrame || this.__rafId !== null) return;
      this.__rafId = requestAnimationFrame(() => {
        this.__rafId = null;
        if (this.paused) return;
        const now = performance.now();
        const elapsed = this.__lastPumpTime === null ? 0 : Math.max(0, now - this.__lastPumpTime) * Math.max(0, this.playbackRate);
        this.__lastPumpTime = now;
        if (this.__frames.length > 1 && this.duration > 0) {
          this.currentTime += elapsed / 1000;
          if (this.currentTime >= this.duration) {
            if (this.loop) {
              this.currentTime %= this.duration;
            } else {
              this.currentTime = this.duration;
              this.ended = true;
            }
          }
          const nextFrame = this.__frameIndexForTime(this.currentTime);
          if (nextFrame !== this.__frameIndex) this.__setFrame(nextFrame);
        }
        this.__notifyFrame();
        if (this.ended && !this.loop) {
          this.paused = true;
          this.dispatchEvent(new Event('ended'));
        } else if (this.__frames.length <= 1 && !this.loop) {
          this.ended = true;
          this.paused = true;
          this.dispatchEvent(new Event('ended'));
        } else if (this.loop && this.__frames.length <= 1) {
          this.__pump();
        } else {
          this.__pump();
        }
      });
    }
    play() {
      this.paused = false;
      this.ended = false;
      return this.__loadPromise.then(() => {
        if (this.readyState >= this.HAVE_CURRENT_DATA) {
          this.__lastPumpTime = performance.now();
          this.dispatchEvent(new Event('play'));
          this.__notifyFrame();
          this.__pump();
        }
      });
    }
    pause() {
      this.paused = true;
      if (this.__rafId !== null) cancelAnimationFrame(this.__rafId);
      this.__rafId = null;
      this.dispatchEvent(new Event('pause'));
    }
    fastSeek(time) {
      this.ended = false;
      this.currentTime = time;
      this.dispatchEvent(new Event('seeked'));
    }
    requestVideoFrameCallback(callback) {
      if (typeof callback !== 'function') throw new TypeError('video frame callback must be a function');
      const id = ++this.__nextFrameCallbackId;
      this.__frameCallbacks.set(id, callback);
      this.__pump();
      return id;
    }
    cancelVideoFrameCallback(id) { this.__frameCallbacks.delete(id); }
  };
  globalThis.HTMLImageElement = globalThis.HTMLImageElement || class HTMLImageElement {
    constructor() {
      this.__listeners = new Map();
      this.__loadToken = 0;
      this.__loadPromise = Promise.resolve();
      this.__loadError = null;
      this._src = '';
      this.crossOrigin = null;
      this.width = 0;
      this.height = 0;
      this.naturalWidth = 0;
      this.naturalHeight = 0;
      this.data = new Uint8Array(0);
      this.complete = false;
      this.__loadError = null;
      this.alt = '';
    }
    get src() { return this._src; }
    set src(value) {
      const token = ++this.__loadToken;
      this._src = String(value);
      this.complete = false;
      this.width = 0;
      this.height = 0;
      this.naturalWidth = 0;
      this.naturalHeight = 0;
      this.data = new Uint8Array(0);
      if (!this._src) {
        this.__loadPromise = Promise.resolve();
        return;
      }
      this.__loadPromise = fetch(this._src)
        .then(response => response.blob())
        .then(blob => createImageBitmap(blob))
        .then(bitmap => {
          if (token !== this.__loadToken) return;
          this.width = bitmap.width;
          this.height = bitmap.height;
          this.naturalWidth = bitmap.width;
          this.naturalHeight = bitmap.height;
          this.data = new Uint8Array(bitmap.data);
          this.complete = true;
          bitmap.close?.();
          this.dispatchEvent(new Event('load'));
        })
        .catch(error => {
          if (token !== this.__loadToken) return;
          this.complete = true;
          this.__loadError = error;
          this.dispatchEvent(Object.assign(new Event('error'), { error }));
        });
    }
    get currentSrc() { return this._src; }
    addEventListener(type, listener) {
      if (typeof listener !== 'function') return;
      const listeners = this.__listeners.get(type) || new Set();
      listeners.add(listener);
      this.__listeners.set(type, listeners);
    }
    removeEventListener(type, listener) { this.__listeners.get(type)?.delete(listener); }
    dispatchEvent(event) {
      for (const listener of this.__listeners.get(event.type) || []) listener.call(this, event);
      const handler = this['on' + event.type];
      if (typeof handler === 'function') handler.call(this, event);
      return !event.defaultPrevented;
    }
    decode() {
      return this.__loadPromise.then(() => {
        if (this.__loadError) throw this.__loadError;
      });
    }
    setAttribute(name, value) {
      const normalized = String(name).toLowerCase();
      if (normalized === 'src') this.src = value;
      else if (normalized === 'crossorigin') this.crossOrigin = value;
      else this[normalized] = value;
    }
    getAttribute(name) {
      const normalized = String(name).toLowerCase();
      if (normalized === 'src') return this._src;
      if (normalized === 'crossorigin') return this.crossOrigin;
      return this[normalized] ?? null;
    }
  };
  globalThis.Image = globalThis.Image || globalThis.HTMLImageElement;
  globalThis.ImageBitmap = globalThis.ImageBitmap || function ImageBitmap() {};
  globalThis.VideoFrame = globalThis.VideoFrame || function VideoFrame() {};
  globalThis.ImageData = globalThis.ImageData || class ImageData {
    constructor(dataOrWidth, widthOrHeight, height) {
      if (typeof dataOrWidth === 'number') {
        this.width = dataOrWidth;
        this.height = Number(widthOrHeight);
        this.data = new Uint8ClampedArray(this.width * this.height * 4);
      } else {
        this.data = new Uint8ClampedArray(dataOrWidth || 0);
        this.width = Number(widthOrHeight);
        this.height = height === undefined ? this.data.length / 4 / this.width : Number(height);
      }
      if (!Number.isInteger(this.width) || !Number.isInteger(this.height) || this.width <= 0 || this.height <= 0 || this.data.length !== this.width * this.height * 4) {
        throw new TypeError('ImageData dimensions do not match RGBA data');
      }
    }
  };
  globalThis.OffscreenCanvas = globalThis.OffscreenCanvas || function OffscreenCanvas() {};
  globalThis.GPUBufferUsage = globalThis.GPUBufferUsage || bufferUsage;
  globalThis.GPUTextureUsage = globalThis.GPUTextureUsage || textureUsage;
  const makeBuffer = (descriptor = {}) => {
    const size = descriptor.size ?? 1;
    const usage = descriptor.usage ?? GPUBufferUsage.COPY_DST;
    const id = __hyperthreeWebGpuCreateBuffer(size, usage);
    const mapped = descriptor.mappedAtCreation === true;
    let shadow = mapped ? new ArrayBuffer(size) : null;
    let mappedForRead = false;
    let mappedOffset = mapped ? 0 : null;
    let mappedSize = mapped ? size : 0;
    let bufferHandle;
    bufferHandle = makeHandle(id, {
      size,
      usage,
      mapState: mapped ? 'mapped' : 'unmapped',
      getMappedRange(offset = 0, rangeSize) {
        if (shadow === null) throw new TypeError('GPUBuffer is not mapped');
        if (rangeSize === undefined) rangeSize = mappedSize - (offset - (mappedOffset ?? 0));
        if (mappedOffset === null || offset < mappedOffset || offset + rangeSize > mappedOffset + mappedSize) {
          throw new RangeError('GPUBuffer mapped range is outside the active mapping');
        }
        const relativeOffset = offset - mappedOffset;
        return relativeOffset === 0 && rangeSize === mappedSize
          ? shadow
          : shadow.slice(relativeOffset, relativeOffset + rangeSize);
      },
      mapAsync: async (mode = GPUMapMode.READ, offset = 0, rangeSize = size - offset) => {
        if (shadow !== null) throw new TypeError('GPUBuffer is already mapped');
        mappedOffset = offset;
        mappedSize = rangeSize;
        if ((mode & GPUMapMode.READ) !== 0) {
          shadow = __hyperthreeWebGpuReadBuffer(id, offset, rangeSize);
          mappedForRead = true;
        } else {
          shadow = new ArrayBuffer(rangeSize);
          mappedForRead = false;
        }
        bufferHandle.mapState = 'mapped';
      },
      unmap() {
        if (shadow !== null && !mappedForRead) {
          __hyperthreeWebGpuWriteBuffer(id, mappedOffset ?? 0, new Uint8Array(shadow));
        }
        shadow = null;
        mappedForRead = false;
        mappedOffset = null;
        mappedSize = 0;
        bufferHandle.mapState = 'unmapped';
      },
      destroy: () => __hyperthreeWebGpuDestroyBuffer(id),
    });
    return bufferHandle;
  };
  const makeTexture = (descriptor = {}) => {
    const size = descriptor.size || {};
    const width = typeof size === 'number' ? size : (Array.isArray(size) ? (size[0] ?? 1) : (size.width ?? 1));
    const height = typeof size === 'number' ? 1 : (Array.isArray(size) ? (size[1] ?? 1) : (size.height ?? 1));
    const depth = typeof size === 'number' || Array.isArray(size) ? (Array.isArray(size) ? (size[2] ?? 1) : 1) : (size.depthOrArrayLayers ?? size.depth ?? 1);
    const format = descriptor.format ?? 'rgba8unorm';
    const usage = descriptor.usage ?? GPUTextureUsage.TEXTURE_BINDING;
    const mipLevelCount = descriptor.mipLevelCount ?? 1;
    const sampleCount = descriptor.sampleCount ?? 1;
    const dimension = descriptor.dimension ?? '2d';
    const id = __hyperthreeWebGpuCreateTexture(
      width,
      height,
      depth,
      format,
      usage,
      mipLevelCount,
      sampleCount,
      dimension,
    );
    return makeHandle(id, {
      width,
      height,
      depthOrArrayLayers: depth,
      mipLevelCount,
      sampleCount,
      dimension,
      format,
      usage,
      createView: (viewDescriptor = {}) => makeHandle(__hyperthreeWebGpuCreateTextureView(id, descriptorJson(viewDescriptor)), { __textureView: true }),
      destroy: () => __hyperthreeWebGpuDestroyTexture(id),
    });
  };
  const makeSurfaceTexture = () => {
    const id = __hyperthreeWebGpuGetCurrentTexture();
    return makeHandle(id, {
      __surfaceTexture: true,
      createView: (viewDescriptor = {}) => makeHandle(__hyperthreeWebGpuCreateTextureView(id, descriptorJson(viewDescriptor)), { __textureView: true }),
      destroy: () => __hyperthreeWebGpuDiscardSurfaceTexture(id),
    });
  };
  const canvasContext = {
    configure(configuration = {}) {
      __hyperthreeWebGpuConfigureCanvas(JSON.stringify({
        format: configuration.format,
        alphaMode: configuration.alphaMode,
        usage: configuration.usage,
      }));
      canvasContext.configuration = configuration;
    },
    unconfigure() { __hyperthreeWebGpuUnconfigureCanvas(); canvasContext.configuration = null; },
    getConfiguration() {
      const nativeConfiguration = JSON.parse(__hyperthreeWebGpuGetCanvasConfiguration());
      return Object.assign({}, canvasContext.configuration || {}, nativeConfiguration);
    },
    getCurrentTexture: makeSurfaceTexture,
  };
  const nativeCanvas = {
    style: {},
    clientWidth: 1280,
    clientHeight: 720,
    getContext(type) { return type === 'webgpu' ? canvasContext : null; },
    addEventListener(type, listener) { globalThis.addEventListener(type, listener); },
    removeEventListener(type, listener) { globalThis.removeEventListener(type, listener); },
    requestPointerLock() { __hyperthreeRequestPointerLock(); },
    requestFullscreen() { __hyperthreeRequestFullscreen(); return Promise.resolve(); },
    setPointerCapture() {},
    releasePointerCapture() {},
    setAttribute() {},
    getAttribute() { return null; },
  };
  let canvasWidth = 1280;
  let canvasHeight = 720;
  Object.defineProperties(nativeCanvas, {
    width: {
      configurable: true,
      get: () => canvasWidth,
      set: value => { canvasWidth = Math.max(1, Math.floor(Number(value) || 1)); __hyperthreeWebGpuResizeCanvas(canvasWidth, canvasHeight); },
    },
    height: {
      configurable: true,
      get: () => canvasHeight,
      set: value => { canvasHeight = Math.max(1, Math.floor(Number(value) || 1)); __hyperthreeWebGpuResizeCanvas(canvasWidth, canvasHeight); },
    },
  });
  Object.setPrototypeOf(nativeCanvas, HTMLCanvasElement.prototype);
  globalThis.__hyperthreeNativeCanvas = nativeCanvas;
  globalThis.document = globalThis.document || {
    createElement(name) {
      if (name === 'canvas') return nativeCanvas;
      if (name === 'video') return new HTMLVideoElement();
      if (name === 'img' || name === 'image') return new HTMLImageElement();
      return { style: {}, addEventListener() {}, removeEventListener() {} };
    },
    createElementNS(_namespace, name) { return this.createElement(String(name).toLowerCase()); },
    body: { appendChild() {}, removeChild() {} },
  };
  globalThis.document.exitPointerLock = () => { __hyperthreeExitPointerLock(); };
  globalThis.document.exitFullscreen = () => { __hyperthreeExitFullscreen(); return Promise.resolve(); };
  Object.defineProperty(globalThis.document, 'pointerLockElement', {
    configurable: true,
    get: () => __hyperthreeIsPointerLocked() ? nativeCanvas : null,
  });
  Object.defineProperty(globalThis.document, 'fullscreenElement', {
    configurable: true,
    get: () => __hyperthreeIsFullscreen() ? nativeCanvas : null,
  });
  const normalizeBindGroupEntry = entry => {
    const resource = entry.resource;
    let normalizedResource;
    if (resource?.__textureView) normalizedResource = { kind: 'textureView', view: handleId(resource) };
    else if (resource?.__sampler) normalizedResource = { kind: 'sampler', sampler: handleId(resource) };
    else if (resource?.buffer) normalizedResource = {
      kind: 'buffer', buffer: handleId(resource.buffer), offset: resource.offset ?? 0, size: resource.size ?? 0,
    };
    else throw new TypeError('unsupported GPUBindGroup resource');
    return { binding: entry.binding, resource: normalizedResource };
  };
  const normalizePipelineDescriptor = descriptor => ({
    layout: descriptor.layout === 'auto' || descriptor.layout == null ? null : handleId(descriptor.layout),
    vertex: {
      module: handleId(descriptor.vertex?.module),
      entryPoint: descriptor.vertex?.entryPoint ?? 'main',
      buffers: (descriptor.vertex?.buffers ?? []).map(buffer => ({
        arrayStride: buffer.arrayStride ?? 0,
        stepMode: buffer.stepMode ?? 'vertex',
        attributes: (buffer.attributes ?? []).map(attribute => ({
          format: attribute.format,
          offset: attribute.offset ?? 0,
          shaderLocation: attribute.shaderLocation,
        })),
      })),
    },
    fragment: descriptor.fragment ? {
      module: handleId(descriptor.fragment.module),
      entryPoint: descriptor.fragment.entryPoint ?? 'main',
      targets: (descriptor.fragment.targets ?? []).map(target => target == null ? null : ({
        format: target.format,
        blend: target.blend,
        writeMask: target.writeMask ?? 15,
      })),
    } : null,
    primitive: descriptor.primitive,
    depthStencil: descriptor.depthStencil,
    multisample: descriptor.multisample ? {
      count: descriptor.multisample.count ?? 1,
      mask: descriptor.multisample.mask ?? 0xffffffff,
      alphaToCoverageEnabled: descriptor.multisample.alphaToCoverageEnabled ?? false,
    } : null,
  });
  const normalizeRenderPassDescriptor = descriptor => ({
    colorAttachments: (descriptor.colorAttachments ?? []).map(attachment => attachment == null ? null : ({
      view: handleId(attachment.view),
      resolveTarget: attachment.resolveTarget ? handleId(attachment.resolveTarget) : null,
      loadOp: attachment.loadOp ?? 'load',
      storeOp: attachment.storeOp ?? 'store',
      clearValue: attachment.clearValue ?? { r: 0, g: 0, b: 0, a: 0 },
    })),
    depthStencilAttachment: descriptor.depthStencilAttachment ? {
      view: handleId(descriptor.depthStencilAttachment.view),
      depthLoadOp: descriptor.depthStencilAttachment.depthLoadOp ?? 'load',
      depthStoreOp: descriptor.depthStencilAttachment.depthStoreOp ?? 'store',
      depthClearValue: descriptor.depthStencilAttachment.depthClearValue ?? 1,
    } : null,
    occlusionQuerySet: descriptor.occlusionQuerySet ? handleId(descriptor.occlusionQuerySet) : null,
    timestampWrites: descriptor.timestampWrites ? {
      querySet: handleId(descriptor.timestampWrites.querySet),
      beginningOfPassWriteIndex: descriptor.timestampWrites.beginningOfPassWriteIndex,
      endOfPassWriteIndex: descriptor.timestampWrites.endOfPassWriteIndex,
    } : null,
  });
  const nativeFeatures = new Set([__HYPERTHREE_FEATURES__]);
  const nativeLimits = __HYPERTHREE_LIMITS__;
  const makeDevice = (descriptor = {}) => {
    for (const feature of descriptor.requiredFeatures ?? []) {
      if (!nativeFeatures.has(feature)) throw new TypeError(`required WebGPU feature is unavailable: ${feature}`);
    }
    for (const [name, requested] of Object.entries(descriptor.requiredLimits ?? {})) {
      if (!(name in nativeLimits) || typeof requested !== 'number' || requested > nativeLimits[name]) {
        throw new TypeError(`required WebGPU limit is unavailable: ${name}`);
      }
    }
    let resolveLost;
    const lost = new Promise(resolve => { resolveLost = resolve; });
    const pollLost = () => {
      const event = __hyperthreeWebGpuPollDeviceLost();
      if (event !== null) resolveLost(JSON.parse(event));
      else requestAnimationFrame(pollLost);
    };
    requestAnimationFrame(pollLost);
    const byteView = value => value instanceof ArrayBuffer
      ? new Uint8Array(value)
      : new Uint8Array(value.buffer, value.byteOffset ?? 0, value.byteLength ?? value.length);
    const queue = {
      writeBuffer(buffer, offset, data, dataOffset = 0, size) {
        const view = byteView(data);
        const elementSize = data.BYTES_PER_ELEMENT ?? 1;
        const start = dataOffset * elementSize;
        const end = size === undefined ? view.byteLength : start + size * elementSize;
        const bytes = view.subarray(start, end);
        __hyperthreeWebGpuWriteBuffer(buffer.__hyperthreeHandle, offset, bytes);
      },
      writeTexture(destination, data, dataLayout, size) {
        const view = byteView(data);
        const bytes = view.subarray(dataLayout.offset ?? 0);
        const origin = destination.origin ?? { x: 0, y: 0, z: 0 };
        const normalizedSize = Array.isArray(size) ? { width: size[0], height: size[1] ?? 1, depthOrArrayLayers: size[2] ?? 1 } : size;
        __hyperthreeWebGpuWriteTexture(
          destination.texture.__hyperthreeHandle,
          normalizedSize.width,
          normalizedSize.height,
          normalizedSize.depthOrArrayLayers ?? 1,
          dataLayout.bytesPerRow ?? normalizedSize.width * 4,
          dataLayout.rowsPerImage ?? normalizedSize.height,
          JSON.stringify([origin.x ?? 0, origin.y ?? 0, origin.z ?? 0]),
          destination.mipLevel ?? 0,
          bytes,
        );
      },
      copyExternalImageToTexture(source, destination, size) {
        const image = source.source;
        if (!image || !image.data) throw new TypeError('external image source has no RGBA data');
        const width = size.width;
        const height = size.height;
        const depth = size.depthOrArrayLayers ?? 1;
        let bytes = image.data;
        if (source.flipY === true) {
          const flipped = new Uint8Array(width * height * 4);
          for (let row = 0; row < height; row += 1) {
            const sourceOffset = row * width * 4;
            const targetOffset = (height - row - 1) * width * 4;
            flipped.set(bytes.subarray(sourceOffset, sourceOffset + width * 4), targetOffset);
          }
          bytes = flipped;
        }
        const origin = destination.origin ?? { x: 0, y: 0, z: 0 };
        __hyperthreeWebGpuWriteTexture(
          destination.texture.__hyperthreeHandle,
          width,
          height,
          depth,
          width * 4,
          height,
          JSON.stringify([origin.x ?? 0, origin.y ?? 0, origin.z ?? 0]),
          destination.mipLevel ?? 0,
          bytes,
        );
      },
      submit(commandBuffers) { __hyperthreeWebGpuSubmit(JSON.stringify((commandBuffers ?? []).map(handleId))); },
      onSubmittedWorkDone: async () => { __hyperthreeWebGpuWaitForSubmittedWork(); },
    };
    const device = {
      queue,
      features: new Set(nativeFeatures),
      limits: nativeLimits,
      createBuffer: makeBuffer,
      createTexture: makeTexture,
      importExternalTexture(descriptor = {}) {
        const source = descriptor.source;
        const width = source?.codedWidth ?? source?.displayWidth ?? source?.width;
        const height = source?.codedHeight ?? source?.displayHeight ?? source?.height;
        if (!source || !Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0 || !source.data) {
          throw new TypeError('native external texture source must provide width, height, and RGBA data');
        }
        const texture = makeTexture({
          size: { width, height },
          format: 'rgba8unorm',
          usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.TEXTURE_BINDING,
        });
        queue.writeTexture(
          { texture },
          source.data,
          { offset: 0, bytesPerRow: width * 4, rowsPerImage: height },
          { width, height, depthOrArrayLayers: 1 },
        );
        const view = texture.createView();
        return makeHandle(handleId(view), {
          __textureView: true,
          __externalTexture: true,
          __externalBackingTexture: texture,
          destroy: () => texture.destroy(),
        });
      },
      createShaderModule(descriptor = {}) {
        return makeHandle(__hyperthreeWebGpuCreateShaderModule(descriptor.code ?? ''));
      },
      createBindGroupLayout(descriptor = {}) {
        return makeHandle(__hyperthreeWebGpuCreateBindGroupLayout(descriptorJson(descriptor)));
      },
      createPipelineLayout(descriptor = {}) {
        const normalized = { bindGroupLayouts: (descriptor.bindGroupLayouts ?? []).map(handleId) };
        return makeHandle(__hyperthreeWebGpuCreatePipelineLayout(descriptorJson(normalized)));
      },
      createBindGroup(descriptor = {}) {
        const normalized = {
          layout: handleId(descriptor.layout),
          entries: (descriptor.entries ?? []).map(normalizeBindGroupEntry),
        };
        return makeHandle(__hyperthreeWebGpuCreateBindGroup(descriptorJson(normalized)));
      },
      createSampler(descriptor = {}) {
        const id = __hyperthreeWebGpuCreateSampler(descriptorJson(descriptor));
        return makeHandle(id, { __sampler: true, destroy: () => __hyperthreeWebGpuDestroySampler(id) });
      },
      createRenderPipeline(descriptor = {}) {
        const id = __hyperthreeWebGpuCreateRenderPipeline(descriptorJson(normalizePipelineDescriptor(descriptor)));
        return makeHandle(id, {
          getBindGroupLayout: (index = 0) => makeHandle(__hyperthreeWebGpuGetRenderPipelineBindGroupLayout(id, index)),
        });
      },
      createRenderPipelineAsync: async (descriptor = {}) => device.createRenderPipeline(descriptor),
      createComputePipeline(descriptor = {}) {
        const normalized = {
          layout: descriptor.layout === 'auto' || descriptor.layout == null ? null : handleId(descriptor.layout),
          compute: {
            module: handleId(descriptor.compute?.module),
            entryPoint: descriptor.compute?.entryPoint ?? 'main',
          },
        };
        const id = __hyperthreeWebGpuCreateComputePipeline(descriptorJson(normalized));
        return makeHandle(id, {
          getBindGroupLayout: (index = 0) => makeHandle(__hyperthreeWebGpuGetComputePipelineBindGroupLayout(id, index)),
        });
      },
      createComputePipelineAsync: async (descriptor = {}) => device.createComputePipeline(descriptor),
      createQuerySet(descriptor = {}) {
        const id = __hyperthreeWebGpuCreateQuerySet(descriptorJson({
          type: descriptor.type,
          count: descriptor.count,
        }));
        return makeHandle(id, { destroy: () => __hyperthreeWebGpuDestroyQuerySet(id) });
      },
      createRenderBundleEncoder(descriptor = {}) {
        const id = __hyperthreeWebGpuCreateRenderBundleEncoder(descriptorJson({
          colorFormats: descriptor.colorFormats ?? [],
          depthStencilFormat: descriptor.depthStencilFormat,
          depthReadOnly: descriptor.depthReadOnly,
          stencilReadOnly: descriptor.stencilReadOnly,
          sampleCount: descriptor.sampleCount ?? 1,
        }));
        return makeRenderBundleEncoder(id);
      },
      createCommandEncoder() {
        const encoderId = __hyperthreeWebGpuCreateCommandEncoder();
        return makeHandle(encoderId, {
          beginRenderPass(descriptor = {}) {
            const passId = __hyperthreeWebGpuBeginRenderPass(encoderId, descriptorJson(normalizeRenderPassDescriptor(descriptor)));
            return makePass(passId);
          },
          beginComputePass(descriptor = {}) {
            const normalized = descriptor.timestampWrites ? {
              timestampWrites: {
                querySet: handleId(descriptor.timestampWrites.querySet),
                beginningOfPassWriteIndex: descriptor.timestampWrites.beginningOfPassWriteIndex,
                endOfPassWriteIndex: descriptor.timestampWrites.endOfPassWriteIndex,
              },
            } : {};
            return makePass(__hyperthreeWebGpuBeginComputePass(encoderId, descriptorJson(normalized)));
          },
          copyBufferToBuffer(source, sourceOffset, destination, destinationOffset, size) {
            __hyperthreeWebGpuEncoderCommand(encoderId, 'copyBufferToBuffer', JSON.stringify([
              handleId(source), sourceOffset ?? 0, handleId(destination), destinationOffset ?? 0, size,
            ]));
          },
          clearBuffer(buffer, offset = 0, size) {
            __hyperthreeWebGpuEncoderCommand(encoderId, 'clearBuffer', JSON.stringify([
              handleId(buffer), offset, size === undefined ? null : size,
            ]));
          },
          copyBufferToTexture(source, destination, size) {
            const normalizeSource = value => ({
              buffer: handleId(value.buffer),
              offset: value.offset ?? 0,
              bytesPerRow: value.bytesPerRow,
              rowsPerImage: value.rowsPerImage ?? (Array.isArray(size) ? size[1] : size.height ?? 1),
            });
            const normalizeDestination = value => ({
              texture: handleId(value.texture),
              mipLevel: value.mipLevel ?? 0,
              origin: Array.isArray(value.origin) ? value.origin : [value.origin?.x ?? 0, value.origin?.y ?? 0, value.origin?.z ?? 0],
            });
            const normalizedSize = Array.isArray(size) ? size : [size.width, size.height ?? 1, size.depthOrArrayLayers ?? 1];
            __hyperthreeWebGpuEncoderCommand(encoderId, 'copyBufferToTexture', JSON.stringify([
              normalizeSource(source), normalizeDestination(destination), normalizedSize,
            ]));
          },
          copyTextureToTexture(source, destination, size) {
            const normalizeCopy = value => ({
              texture: handleId(value.texture),
              mipLevel: value.mipLevel ?? 0,
              origin: Array.isArray(value.origin) ? value.origin : [value.origin?.x ?? 0, value.origin?.y ?? 0, value.origin?.z ?? 0],
            });
            const normalizedSize = Array.isArray(size) ? size : [size.width, size.height ?? 1, size.depthOrArrayLayers ?? 1];
            __hyperthreeWebGpuEncoderCommand(encoderId, 'copyTextureToTexture', JSON.stringify([
              normalizeCopy(source), normalizeCopy(destination), normalizedSize,
            ]));
          },
          copyTextureToBuffer(source, destination, size) {
            const normalizeSource = value => ({
              texture: handleId(value.texture),
              mipLevel: value.mipLevel ?? 0,
              origin: Array.isArray(value.origin) ? value.origin : [value.origin?.x ?? 0, value.origin?.y ?? 0, value.origin?.z ?? 0],
            });
            const normalizedSize = Array.isArray(size) ? size : [size.width, size.height ?? 1, size.depthOrArrayLayers ?? 1];
            __hyperthreeWebGpuEncoderCommand(encoderId, 'copyTextureToBuffer', JSON.stringify([
              normalizeSource(source),
              {
                buffer: handleId(destination.buffer),
                offset: destination.offset ?? 0,
                bytesPerRow: destination.bytesPerRow,
                rowsPerImage: destination.rowsPerImage ?? normalizedSize[1],
              },
              normalizedSize,
            ]));
          },
          resolveQuerySet(querySet, firstQuery, queryCount, destination, destinationOffset) {
            __hyperthreeWebGpuEncoderCommand(encoderId, 'resolveQuerySet', JSON.stringify([
              handleId(querySet), firstQuery, queryCount, handleId(destination), destinationOffset ?? 0,
            ]));
          },
          writeTimestamp(querySet, queryIndex) {
            __hyperthreeWebGpuEncoderCommand(encoderId, 'writeTimestamp', JSON.stringify([
              handleId(querySet), queryIndex,
            ]));
          },
          finish() { return makeHandle(__hyperthreeWebGpuFinishCommandEncoder(encoderId)); },
        });
      },
      pushErrorScope(filter) {
        __hyperthreeWebGpuPushErrorScope(filter);
      },
      async popErrorScope() {
        const error = __hyperthreeWebGpuPopErrorScope();
        return error === null ? null : { message: error };
      },
      lost,
      destroy: () => __hyperthreeWebGpuDestroyDevice(),
    };
    return device;
  };
  const makePass = passId => {
    const command = (operation, args) => __hyperthreeWebGpuPassCommand(passId, operation, JSON.stringify(args));
    return {
      setPipeline(pipeline) { command('setPipeline', [handleId(pipeline)]); },
      setBindGroup(index, bindGroup, dynamicOffsets = []) { command('setBindGroup', [index, handleId(bindGroup), dynamicOffsets]); },
      setVertexBuffer(slot, buffer, offset = 0, size) { command('setVertexBuffer', [slot, handleId(buffer), offset, size ?? null]); },
      setIndexBuffer(buffer, format, offset = 0) { command('setIndexBuffer', [handleId(buffer), offset, format]); },
      setViewport(x, y, width, height, minDepth, maxDepth) { command('setViewport', [x, y, width, height, minDepth, maxDepth]); },
      setScissorRect(x, y, width, height) { command('setScissorRect', [x, y, width, height]); },
      setBlendConstant(value) { command('setBlendConstant', [value.r, value.g, value.b, value.a]); },
      setStencilReference(value) { command('setStencilReference', [value]); },
      draw(vertexCount, instanceCount = 1, firstVertex = 0, firstInstance = 0) { command('draw', [vertexCount, instanceCount, firstVertex, firstInstance]); },
      drawIndexed(indexCount, instanceCount = 1, firstIndex = 0, baseVertex = 0, firstInstance = 0) { command('drawIndexed', [indexCount, instanceCount, firstIndex, baseVertex, firstInstance]); },
      drawIndirect(buffer, offset = 0) { command('drawIndirect', [handleId(buffer), offset]); },
      drawIndexedIndirect(buffer, offset = 0) { command('drawIndexedIndirect', [handleId(buffer), offset]); },
      beginOcclusionQuery(queryIndex) { command('beginOcclusionQuery', [queryIndex]); },
      endOcclusionQuery() { command('endOcclusionQuery', []); },
      executeBundles(bundles = []) { command('executeBundles', bundles.map(handleId)); },
      dispatchWorkgroups(x, y = 1, z = 1) { command('dispatchWorkgroups', [x, y, z]); },
      dispatchWorkgroupsIndirect(buffer, offset = 0) { command('dispatchWorkgroupsIndirect', [handleId(buffer), offset]); },
      end() { __hyperthreeWebGpuEndPass(passId); },
      endPass() { __hyperthreeWebGpuEndPass(passId); },
    };
  };
  const makeRenderBundleEncoder = encoderId => {
    const command = (operation, args) => __hyperthreeWebGpuRenderBundleCommand(encoderId, operation, JSON.stringify(args));
    return {
      setPipeline(pipeline) { command('setPipeline', [handleId(pipeline)]); },
      setBindGroup(index, bindGroup, dynamicOffsets = []) { command('setBindGroup', [index, handleId(bindGroup), dynamicOffsets]); },
      setVertexBuffer(slot, buffer, offset = 0, size) { command('setVertexBuffer', [slot, handleId(buffer), offset, size ?? null]); },
      setIndexBuffer(buffer, format, offset = 0) { command('setIndexBuffer', [handleId(buffer), offset, format]); },
      // Three.js' common backend calls these state setters while recording a
      // bundle. WebGPU render bundles inherit this state from the render pass,
      // so the native pass remains authoritative and these are compatibility no-ops.
      setViewport() {},
      setScissorRect() {},
      setBlendConstant() {},
      setStencilReference() {},
      draw(vertexCount, instanceCount = 1, firstVertex = 0, firstInstance = 0) { command('draw', [vertexCount, instanceCount, firstVertex, firstInstance]); },
      drawIndexed(indexCount, instanceCount = 1, firstIndex = 0, baseVertex = 0, firstInstance = 0) { command('drawIndexed', [indexCount, instanceCount, firstIndex, baseVertex, firstInstance]); },
      finish() {
        const id = __hyperthreeWebGpuFinishRenderBundleEncoder(encoderId);
        return makeHandle(id, { destroy: () => __hyperthreeWebGpuDestroyRenderBundle(id) });
      },
    };
  };
  const adapter = {
    name: 'HyperThree Native wgpu',
    features: new Set(nativeFeatures),
    limits: nativeLimits,
    info: { vendor: 'HyperThree', architecture: 'wgpu', device: 'native', description: 'HyperThree Native wgpu adapter' },
    isFallbackAdapter: false,
    requestDevice: async (descriptor = {}) => makeDevice(descriptor),
    requestAdapterInfo: async () => adapter.info,
  };
  globalThis.navigator = globalThis.navigator || {};
  globalThis.navigator.userAgent = globalThis.navigator.userAgent || 'HyperThreeNative/0.1';
  globalThis.navigator.platform = globalThis.navigator.platform || 'HyperThreeNative';
  globalThis.navigator.language = globalThis.navigator.language || 'en-US';
  globalThis.window = globalThis.window || globalThis;
  globalThis.window.devicePixelRatio = globalThis.window.devicePixelRatio || 1;
  globalThis.window.innerWidth = globalThis.window.innerWidth || nativeCanvas.width;
  globalThis.window.innerHeight = globalThis.window.innerHeight || nativeCanvas.height;
  globalThis.navigator.gpu = {
    requestAdapter: async (_options = {}) => adapter,
    getPreferredCanvasFormat: () => 'bgra8unorm-srgb',
  };
})();
"#;

#[cfg(test)]
mod tests {
    use super::{multisample_state, sanitize_wgsl, texture_format, texture_usage_for_dimension};
    use serde_json::json;

    #[test]
    fn removes_unsupported_wgsl_diagnostic_directives() {
        let source = "diagnostic(off, derivative_uniformity);\n@compute @workgroup_size(1) fn main() {}\n  diagnostic(on, foo);";

        assert_eq!(
            sanitize_wgsl(source),
            "@compute @workgroup_size(1) fn main() {}"
        );
    }

    #[test]
    fn normalizes_three_texture_array_load_level() {
        let source = "nodeVar = textureLoad( texture, coord, layer, u32( 2u ) );";

        assert_eq!(
            sanitize_wgsl(source),
            "nodeVar = textureLoad( texture, coord, layer, i32(2));"
        );
    }

    #[test]
    fn normalizes_abstract_integer_literals_for_naga() {
        let source = "let level = u32( 0.0 ); let layer = i32( 1.0 );";

        assert_eq!(sanitize_wgsl(source), "let level = 0u; let layer = 1i;");
    }

    #[test]
    fn normalizes_external_texture_sampling_for_native_wgpu() {
        let source = "@group(0) @binding(0) var video: texture_external; fn sample() { let color = textureSampleBaseClampToEdge(video, sampler, vec2f(0.5)); }";

        assert_eq!(
            sanitize_wgsl(source),
            "@group(0) @binding(0) var video: texture_2d<f32>; fn sample() { let color = textureSample(video, sampler, vec2f(0.5)); }"
        );
    }

    #[test]
    fn maps_three_compressed_texture_formats_to_native_formats() {
        assert_eq!(
            texture_format("bc7-rgba-unorm-srgb"),
            wgpu::TextureFormat::Bc7RgbaUnormSrgb
        );
        assert_eq!(
            texture_format("etc2-rgba8unorm"),
            wgpu::TextureFormat::Etc2Rgba8Unorm
        );
        assert_eq!(
            texture_format("astc-4x4-unorm-srgb"),
            wgpu::TextureFormat::Astc {
                block: wgpu::AstcBlock::B4x4,
                channel: wgpu::AstcChannel::UnormSrgb,
            }
        );
    }

    #[test]
    fn maps_standard_hdr_and_depth_texture_formats_to_native_formats() {
        assert_eq!(
            texture_format("rgba32float"),
            wgpu::TextureFormat::Rgba32Float
        );
        assert_eq!(
            texture_format("rg11b10ufloat"),
            wgpu::TextureFormat::Rg11b10Float
        );
        assert_eq!(
            texture_format("depth32float-stencil8"),
            wgpu::TextureFormat::Depth32FloatStencil8
        );
    }

    #[test]
    fn removes_invalid_render_attachment_usage_from_non_2d_textures() {
        let usage = texture_usage_for_dimension(2 | 4 | 16, "3d");
        assert!(usage.contains(wgpu::TextureUsages::COPY_DST));
        assert!(usage.contains(wgpu::TextureUsages::TEXTURE_BINDING));
        assert!(!usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
    }

    #[test]
    fn preserves_pipeline_multisample_state() {
        let state = multisample_state(&json!({
            "count": 4,
            "mask": 0x0f0f0f0f_u64,
            "alphaToCoverageEnabled": true,
        }))
        .unwrap();
        assert_eq!(state.count, 4);
        assert_eq!(state.mask, 0x0f0f0f0f);
        assert!(state.alpha_to_coverage_enabled);
    }
}
