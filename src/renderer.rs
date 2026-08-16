use crate::bridge::{
    CameraProjection, CameraSnapshot, GeometryData, GeometryKind, MaterialSnapshot,
    SharedRenderState, TextureData,
};
use crate::webgpu::{NativeWebGpuContext, SharedNativeWebGpuContext};
use anyhow::{Context as _, Result};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
const MAX_INSTANCES: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

const CUBE_VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.5, -0.5, 0.5],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        normal: [0.0, 0.0, -1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [0.5, -0.5, -0.5],
        normal: [0.0, 0.0, -1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        normal: [0.0, 0.0, -1.0],
        uv: [0.0, 0.0],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        normal: [0.0, 0.0, -1.0],
        uv: [1.0, 0.0],
    },
];

const CUBE_INDICES: &[u16] = &[
    0, 1, 2, 2, 3, 0, // front
    1, 5, 6, 6, 2, 1, // right
    5, 4, 7, 7, 6, 5, // back
    4, 0, 3, 3, 7, 4, // left
    3, 2, 6, 6, 7, 3, // top
    4, 5, 1, 1, 0, 4, // bottom
];

const PLANE_VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.5, -0.5, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [0.5, 0.5, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-0.5, 0.5, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.0],
    },
];

const PLANE_INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
    base_color: [f32; 4],
    material: [f32; 4],
    emissive: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    camera_position: [f32; 4],
    light_direction: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
}

struct GpuGeometry {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    source: Arc<GeometryData>,
}

struct CustomBatch {
    geometry_id: u64,
    texture_id: Option<u64>,
    instance_offset: usize,
    instance_count: usize,
}

struct ParticleBatch {
    instance_offset: usize,
    instance_count: usize,
}

pub struct Renderer {
    pub window: Arc<Window>,
    surface: Arc<wgpu::Surface<'static>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    cube_vertex_buffer: wgpu::Buffer,
    cube_index_buffer: wgpu::Buffer,
    plane_vertex_buffer: wgpu::Buffer,
    plane_index_buffer: wgpu::Buffer,
    sphere_vertex_buffer: wgpu::Buffer,
    sphere_index_buffer: wgpu::Buffer,
    sphere_index_count: u32,
    custom_geometries: HashMap<u64, GpuGeometry>,
    textures: HashMap<u64, GpuTexture>,
    instance_buffer: wgpu::Buffer,
    instance_bind_group: wgpu::BindGroup,
    instance_bind_group_layout: wgpu::BindGroupLayout,
    frame_buffer: wgpu::Buffer,
    texture_sampler: wgpu::Sampler,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    render_state: SharedRenderState,
    size: PhysicalSize<u32>,
    webgpu_context: SharedNativeWebGpuContext,
}

struct GpuTexture {
    bind_group: wgpu::BindGroup,
    source: Arc<TextureData>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, render_state: SharedRenderState) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = Arc::new(
            instance
                .create_surface(window.clone())
                .context("failed to create native GPU surface")?,
        );
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no compatible native GPU adapter found")?;
        log::info!("using GPU adapter: {}", adapter.get_info().name);

        let compression_features = wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::TEXTURE_COMPRESSION_ETC2
            | wgpu::Features::TEXTURE_COMPRESSION_ASTC;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("hyperthree-device"),
                    required_features: adapter.features() & compression_features,
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .context("failed to create wgpu device")?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hyperthree-cube-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/cube.wgsl").into()),
        });
        let instance_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hyperthree-instance-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hyperthree-pipeline-layout"),
            bind_group_layouts: &[&instance_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hyperthree-cube-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let cube_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyperthree-cube-vertices"),
            contents: bytemuck::cast_slice(CUBE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let cube_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyperthree-cube-indices"),
            contents: bytemuck::cast_slice(CUBE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let plane_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyperthree-plane-vertices"),
            contents: bytemuck::cast_slice(PLANE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let plane_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyperthree-plane-indices"),
            contents: bytemuck::cast_slice(PLANE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let (sphere_vertices, sphere_indices) = create_sphere_mesh(24, 16);
        let sphere_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyperthree-sphere-vertices"),
            contents: bytemuck::cast_slice(&sphere_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let sphere_index_count = sphere_indices.len() as u32;
        let sphere_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyperthree-sphere-indices"),
            contents: bytemuck::cast_slice(&sphere_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hyperthree-scene-instances"),
            size: (std::mem::size_of::<Instance>() * MAX_INSTANCES) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hyperthree-frame-uniform"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let white_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hyperthree-white-texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let white_texture_view = white_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hyperthree-texture-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &white_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let instance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hyperthree-instance-bind-group"),
            layout: &instance_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&white_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&texture_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: frame_buffer.as_entire_binding(),
                },
            ],
        });
        let (depth_texture, depth_view) = create_depth_resources(&device, &config);
        let webgpu_context = NativeWebGpuContext::new(
            device.clone(),
            queue.clone(),
            surface.clone(),
            config.clone(),
            device.features(),
        );

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            cube_vertex_buffer,
            cube_index_buffer,
            plane_vertex_buffer,
            plane_index_buffer,
            sphere_vertex_buffer,
            sphere_index_buffer,
            sphere_index_count,
            custom_geometries: HashMap::new(),
            textures: HashMap::new(),
            instance_buffer,
            instance_bind_group,
            instance_bind_group_layout,
            frame_buffer,
            texture_sampler,
            depth_texture,
            depth_view,
            render_state,
            size,
            webgpu_context,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        if let Err(error) = self.webgpu_context.resize_surface(size.width, size.height) {
            log::warn!("failed to resize WebGPU compatibility surface: {error}");
        }
        let (depth_texture, depth_view) = create_depth_resources(&self.device, &self.config);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
    }

    pub fn webgpu_context(&self) -> SharedNativeWebGpuContext {
        self.webgpu_context.clone()
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if self.webgpu_context.take_presented_this_frame() {
            return Ok(());
        }
        let snapshot = self
            .render_state
            .lock()
            .expect("render state mutex should not be poisoned")
            .snapshot();
        let instance_count = snapshot.cubes.len().min(MAX_INSTANCES);
        if snapshot.cubes.len() > MAX_INSTANCES {
            log::warn!("native instance limit reached; rendering first {MAX_INSTANCES} cubes");
        }
        let aspect = self.config.width as f32 / self.config.height as f32;
        let light = snapshot.directional_light;
        self.queue.write_buffer(
            &self.frame_buffer,
            0,
            bytemuck::bytes_of(&FrameUniform {
                camera_position: [
                    snapshot.camera.position[0] as f32,
                    snapshot.camera.position[1] as f32,
                    snapshot.camera.position[2] as f32,
                    1.0,
                ],
                light_direction: [
                    light.direction[0] as f32,
                    light.direction[1] as f32,
                    light.direction[2] as f32,
                    light.intensity as f32,
                ],
                light_color: [
                    light.color[0] as f32,
                    light.color[1] as f32,
                    light.color[2] as f32,
                    1.0,
                ],
                ambient: [
                    light.ambient[0] as f32,
                    light.ambient[1] as f32,
                    light.ambient[2] as f32,
                    1.0,
                ],
            }),
        );
        let mut instances = Vec::with_capacity(instance_count);
        let mut batches = [
            (GeometryKind::Cube, 0_usize),
            (GeometryKind::Plane, 0_usize),
            (GeometryKind::Sphere, 0_usize),
        ];
        for geometry in [
            GeometryKind::Cube,
            GeometryKind::Plane,
            GeometryKind::Sphere,
        ] {
            let batch_start = instances.len();
            for mesh in snapshot.cubes[..instance_count]
                .iter()
                .filter(|mesh| mesh.geometry as u8 == geometry as u8)
            {
                instances.push(build_instance(
                    &snapshot.camera,
                    mesh.position,
                    mesh.scale,
                    mesh.rotation_y,
                    mesh.model_matrix,
                    mesh.material,
                    aspect,
                ));
            }
            batches[match geometry {
                GeometryKind::Cube => 0,
                GeometryKind::Plane => 1,
                GeometryKind::Sphere => 2,
            }]
            .1 = instances.len() - batch_start;
        }
        let mut custom_batches = Vec::new();
        let mut custom_instances =
            BTreeMap::<(u64, Option<u64>), Vec<&crate::bridge::CustomMeshSnapshot>>::new();
        for mesh in &snapshot.custom_meshes {
            custom_instances
                .entry((mesh.geometry_id, mesh.texture_id))
                .or_default()
                .push(mesh);
        }
        let registry = snapshot
            .geometry_registry
            .lock()
            .expect("geometry registry mutex should not be poisoned");
        let texture_registry = snapshot
            .texture_registry
            .lock()
            .expect("texture registry mutex should not be poisoned");
        for ((geometry_id, texture_id), meshes) in custom_instances {
            let Some(data) = registry.get(geometry_id) else {
                log::warn!("custom geometry {geometry_id} was not registered");
                continue;
            };
            self.ensure_custom_geometry(geometry_id, data);
            if let Some(texture_id) = texture_id {
                if let Some(texture) = texture_registry.get(texture_id) {
                    self.ensure_texture(texture_id, texture);
                } else {
                    log::warn!("custom texture {texture_id} was not registered");
                }
            }
            let instance_offset = instances.len();
            for mesh in meshes
                .iter()
                .take(MAX_INSTANCES.saturating_sub(instance_offset))
            {
                instances.push(build_instance(
                    &snapshot.camera,
                    mesh.position,
                    mesh.scale,
                    mesh.rotation_y,
                    mesh.model_matrix,
                    mesh.material,
                    aspect,
                ));
            }
            custom_batches.push(CustomBatch {
                geometry_id,
                texture_id,
                instance_offset,
                instance_count: instances.len() - instance_offset,
            });
            if instances.len() >= MAX_INSTANCES {
                break;
            }
        }
        drop(texture_registry);
        drop(registry);
        let particle_offset = instances.len();
        for particle in snapshot
            .particles
            .iter()
            .take(MAX_INSTANCES.saturating_sub(particle_offset))
        {
            let model = billboard_model(&snapshot.camera, particle.position, particle.size);
            instances.push(build_instance_from_model(
                &snapshot.camera,
                &model,
                MaterialSnapshot {
                    base_color: particle.color,
                    metallic: 0.0,
                    roughness: 1.0,
                    emissive: particle.emissive,
                    unlit: true,
                    base_color_texture: None,
                },
                aspect,
            ));
        }
        let particle_batch = ParticleBatch {
            instance_offset: particle_offset,
            instance_count: instances.len() - particle_offset,
        };
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("hyperthree-render-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hyperthree-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: snapshot.clear_color[0],
                            g: snapshot.clear_color[1],
                            b: snapshot.clear_color[2],
                            a: snapshot.clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            let mut instance_offset = 0;
            pass.set_bind_group(0, &self.instance_bind_group, &[]);
            for (geometry, count) in batches {
                if count == 0 {
                    continue;
                }
                let (vertex_buffer, index_buffer, index_count) = match geometry {
                    GeometryKind::Cube => (
                        &self.cube_vertex_buffer,
                        &self.cube_index_buffer,
                        CUBE_INDICES.len() as u32,
                    ),
                    GeometryKind::Plane => (
                        &self.plane_vertex_buffer,
                        &self.plane_index_buffer,
                        PLANE_INDICES.len() as u32,
                    ),
                    GeometryKind::Sphere => (
                        &self.sphere_vertex_buffer,
                        &self.sphere_index_buffer,
                        self.sphere_index_count,
                    ),
                };
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(
                    0..index_count,
                    0,
                    instance_offset as u32..(instance_offset + count) as u32,
                );
                instance_offset += count;
            }
            for batch in custom_batches {
                let Some(geometry) = self.custom_geometries.get(&batch.geometry_id) else {
                    continue;
                };
                if let Some(texture_id) = batch.texture_id {
                    if let Some(texture) = self.textures.get(&texture_id) {
                        pass.set_bind_group(0, &texture.bind_group, &[]);
                    }
                } else {
                    pass.set_bind_group(0, &self.instance_bind_group, &[]);
                }
                pass.set_vertex_buffer(0, geometry.vertex_buffer.slice(..));
                pass.set_index_buffer(geometry.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(
                    0..geometry.index_count,
                    0,
                    batch.instance_offset as u32
                        ..(batch.instance_offset + batch.instance_count) as u32,
                );
            }
            if particle_batch.instance_count > 0 {
                pass.set_bind_group(0, &self.instance_bind_group, &[]);
                pass.set_vertex_buffer(0, self.plane_vertex_buffer.slice(..));
                pass.set_index_buffer(self.plane_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(
                    0..PLANE_INDICES.len() as u32,
                    0,
                    particle_batch.instance_offset as u32
                        ..(particle_batch.instance_offset + particle_batch.instance_count) as u32,
                );
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    fn ensure_custom_geometry(&mut self, geometry_id: u64, data: Arc<GeometryData>) {
        if self
            .custom_geometries
            .get(&geometry_id)
            .is_some_and(|geometry| Arc::ptr_eq(&geometry.source, &data))
        {
            return;
        }
        let vertices: Vec<Vertex> = data
            .positions
            .iter()
            .copied()
            .enumerate()
            .map(|(index, position)| Vertex {
                position,
                normal: data.normals.get(index).copied().unwrap_or([0.0, 1.0, 0.0]),
                uv: data.uvs.get(index).copied().unwrap_or([0.0, 0.0]),
            })
            .collect();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("hyperthree-buffer-geometry-vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("hyperthree-buffer-geometry-indices"),
                contents: bytemuck::cast_slice(&data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.custom_geometries.insert(
            geometry_id,
            GpuGeometry {
                vertex_buffer,
                index_buffer,
                index_count: data.indices.len() as u32,
                source: data,
            },
        );
    }

    fn ensure_texture(&mut self, texture_id: u64, data: Arc<TextureData>) {
        if self
            .textures
            .get(&texture_id)
            .is_some_and(|texture| Arc::ptr_eq(&texture.source, &data))
        {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hyperthree-native-texture"),
            size: wgpu::Extent3d {
                width: data.width,
                height: data.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data.rgba8,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * data.width),
                rows_per_image: Some(data.height),
            },
            wgpu::Extent3d {
                width: data.width,
                height: data.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hyperthree-native-texture-bind-group"),
            layout: &self.instance_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.texture_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.frame_buffer.as_entire_binding(),
                },
            ],
        });
        self.textures.insert(
            texture_id,
            GpuTexture {
                bind_group,
                source: data,
            },
        );
    }
}

fn create_sphere_mesh(segments: u32, rings: u32) -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::with_capacity(((segments + 1) * (rings + 1)) as usize);
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = std::f32::consts::PI * v;
        for segment in 0..=segments {
            let u = segment as f32 / segments as f32;
            let theta = std::f32::consts::TAU * u;
            vertices.push(Vertex {
                position: [theta.sin() * phi.sin(), phi.cos(), theta.cos() * phi.sin()],
                normal: [theta.sin() * phi.sin(), phi.cos(), theta.cos() * phi.sin()],
                uv: [u, v],
            });
        }
    }
    let mut indices = Vec::with_capacity((segments * rings * 6) as usize);
    for ring in 0..rings {
        for segment in 0..segments {
            let first = ring * (segments + 1) + segment;
            let second = first + segments + 1;
            indices.extend_from_slice(&[
                first as u16,
                second as u16,
                (first + 1) as u16,
                second as u16,
                (second + 1) as u16,
                (first + 1) as u16,
            ]);
        }
    }
    (vertices, indices)
}

fn create_depth_resources(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hyperthree-depth-texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn build_mvp_from_model(
    camera: &CameraSnapshot,
    model: &[[f32; 4]; 4],
    aspect: f32,
) -> [[f32; 4]; 4] {
    let eye = vec3(camera.position);
    let target = vec3(camera.target);
    let view = look_at(
        eye,
        target,
        Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
    );
    let projection = match camera.projection {
        CameraProjection::Perspective => perspective(
            (camera.fov_y_degrees as f32).to_radians(),
            aspect,
            camera.near as f32,
            camera.far as f32,
        ),
        CameraProjection::Orthographic {
            left,
            right,
            top,
            bottom,
        } => orthographic(
            left as f32,
            right as f32,
            bottom as f32,
            top as f32,
            camera.near as f32,
            camera.far as f32,
        ),
    };
    mat_mul(&projection, &mat_mul(&view, model))
}

fn build_model_values(
    position: [f64; 3],
    scale_values: [f64; 3],
    rotation_y_value: f64,
) -> [[f32; 4]; 4] {
    mat_mul(
        &translation(vec3(position)),
        &mat_mul(
            &rotation_y(rotation_y_value as f32),
            &scale(vec3(scale_values)),
        ),
    )
}

fn build_instance(
    camera: &CameraSnapshot,
    position: [f64; 3],
    scale: [f64; 3],
    rotation_y: f64,
    model_matrix: Option<[[f64; 4]; 4]>,
    material: MaterialSnapshot,
    aspect: f32,
) -> Instance {
    let model = model_matrix
        .map(|matrix| matrix.map(|column| column.map(|value| value as f32)))
        .unwrap_or_else(|| build_model_values(position, scale, rotation_y));
    build_instance_from_model(camera, &model, material, aspect)
}

fn build_instance_from_model(
    camera: &CameraSnapshot,
    model: &[[f32; 4]; 4],
    material: MaterialSnapshot,
    aspect: f32,
) -> Instance {
    Instance {
        mvp: build_mvp_from_model(camera, model, aspect),
        model: *model,
        normal_matrix: *model,
        base_color: material.base_color.map(|component| component as f32),
        material: [
            material.metallic as f32,
            material.roughness as f32,
            if material.unlit { 1.0 } else { 0.0 },
            0.0,
        ],
        emissive: [
            material.emissive[0] as f32,
            material.emissive[1] as f32,
            material.emissive[2] as f32,
            0.0,
        ],
    }
}

fn billboard_model(camera: &CameraSnapshot, position: [f64; 3], size: f64) -> [[f32; 4]; 4] {
    let eye = vec3(camera.position);
    let target = vec3(camera.target);
    let forward = normalize(sub(target, eye));
    let right = normalize(cross(
        forward,
        Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
    ));
    let up = cross(right, forward);
    let size = size as f32;
    [
        [right.x * size, right.y * size, right.z * size, 0.0],
        [up.x * size, up.y * size, up.z * size, 0.0],
        [-forward.x * size, -forward.y * size, -forward.z * size, 0.0],
        [
            position[0] as f32,
            position[1] as f32,
            position[2] as f32,
            1.0,
        ],
    ]
}

#[derive(Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

fn vec3(value: [f64; 3]) -> Vec3 {
    Vec3 {
        x: value[0] as f32,
        y: value[1] as f32,
        z: value[2] as f32,
    }
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.x - b.x,
        y: a.y - b.y,
        z: a.z - b.z,
    }
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn normalize(value: Vec3) -> Vec3 {
    let length = dot(value, value).sqrt().max(f32::EPSILON);
    Vec3 {
        x: value.x / length,
        y: value.y / length,
        z: value.z / length,
    }
}

fn identity() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mat_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            result[column][row] = (0..4).map(|i| a[i][row] * b[column][i]).sum();
        }
    }
    result
}

fn translation(value: Vec3) -> [[f32; 4]; 4] {
    let mut result = identity();
    result[3][0] = value.x;
    result[3][1] = value.y;
    result[3][2] = value.z;
    result
}

fn scale(value: Vec3) -> [[f32; 4]; 4] {
    [
        [value.x, 0.0, 0.0, 0.0],
        [0.0, value.y, 0.0, 0.0],
        [0.0, 0.0, value.z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn rotation_y(angle: f32) -> [[f32; 4]; 4] {
    let (sin, cos) = angle.sin_cos();
    [
        [cos, 0.0, -sin, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [sin, 0.0, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> [[f32; 4]; 4] {
    let forward = normalize(sub(target, eye));
    let right = normalize(cross(forward, up));
    let up = cross(right, forward);
    [
        [right.x, up.x, -forward.x, 0.0],
        [right.y, up.y, -forward.y, 0.0],
        [right.z, up.z, -forward.z, 0.0],
        [-dot(right, eye), -dot(up, eye), dot(forward, eye), 1.0],
    ]
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let focal = 1.0 / (fov_y / 2.0).tan();
    [
        [focal / aspect, 0.0, 0.0, 0.0],
        [0.0, focal, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, (far * near) / (near - far), 0.0],
    ]
}

fn orthographic(
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    near: f32,
    far: f32,
) -> [[f32; 4]; 4] {
    [
        [2.0 / (right - left), 0.0, 0.0, 0.0],
        [0.0, 2.0 / (top - bottom), 0.0, 0.0],
        [0.0, 0.0, 1.0 / (near - far), 0.0],
        [
            -(right + left) / (right - left),
            -(top + bottom) / (top - bottom),
            near / (near - far),
            1.0,
        ],
    ]
}
