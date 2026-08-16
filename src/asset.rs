use anyhow::{Context, Result};
use memmap2::{Mmap, MmapOptions};
use serde_json::Value;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fs::File,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

/// Memory-mapped asset storage. The mapped bytes can be handed to a native
/// decoder without first copying them through a JavaScript ArrayBuffer.
#[derive(Debug)]
pub struct MappedAsset {
    map: Mmap,
}

#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub relative_path: String,
    pub byte_length: usize,
    pub format: String,
    pub mesh_count: usize,
    pub primitive_count: usize,
    pub animation_count: usize,
}

#[derive(Debug, Clone)]
pub struct AssetGeometry {
    pub geometry_id: u64,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub texture: Option<AssetTexture>,
    pub material: AssetMaterial,
}

#[derive(Debug, Clone, Copy)]
pub struct AssetMaterial {
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    pub emissive: [f64; 3],
    pub unlit: bool,
}

impl Default for AssetMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 1.0,
            emissive: [0.0, 0.0, 0.0],
            unlit: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetTexture {
    pub texture_id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Debug)]
pub struct AssetStore {
    root: PathBuf,
    mapped: HashMap<PathBuf, Arc<MappedAsset>>,
    decoded: HashMap<(PathBuf, usize, usize), Arc<AssetGeometry>>,
}

impl AssetStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("asset root does not exist: {}", root.as_ref().display()))?;
        Ok(Self {
            root,
            mapped: HashMap::new(),
            decoded: HashMap::new(),
        })
    }

    pub fn load(&mut self, relative_path: &str) -> Result<AssetMetadata> {
        let relative = validate_relative_path(relative_path)?;
        let canonical = self.resolve_path(&relative, relative_path)?;
        if !self.mapped.contains_key(&canonical) {
            self.mapped
                .insert(canonical.clone(), Arc::new(MappedAsset::open(&canonical)?));
        }
        let mapped = self
            .mapped
            .get(&canonical)
            .expect("asset mapping was inserted")
            .clone();
        let inspected = inspect_asset(mapped.bytes(), &canonical)?;
        Ok(AssetMetadata {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            byte_length: mapped.len(),
            format: inspected.format,
            mesh_count: inspected.mesh_count,
            primitive_count: inspected.primitive_count,
            animation_count: inspected.animation_count,
        })
    }

    /// Read a project-relative asset for JavaScript APIs such as `fetch()`.
    ///
    /// Native decoders should continue to use `load_geometry()` and the mmap
    /// directly. This copy exists at the browser-compatibility boundary where
    /// the Fetch/ArrayBuffer contract requires JS-owned bytes.
    pub fn read_bytes(&mut self, relative_path: &str) -> Result<Vec<u8>> {
        let relative = validate_relative_path(relative_path)?;
        let canonical = self.resolve_path(&relative, relative_path)?;
        if !self.mapped.contains_key(&canonical) {
            self.mapped
                .insert(canonical.clone(), Arc::new(MappedAsset::open(&canonical)?));
        }
        let mapped = self
            .mapped
            .get(&canonical)
            .expect("asset mapping was inserted")
            .clone();
        Ok(mapped.bytes().to_vec())
    }

    pub fn load_geometry(
        &mut self,
        relative_path: &str,
        mesh_index: usize,
        primitive_index: usize,
    ) -> Result<Arc<AssetGeometry>> {
        let relative = validate_relative_path(relative_path)?;
        let canonical = self.resolve_path(&relative, relative_path)?;
        // Retain the source mapping for the lifetime of the decoded geometry;
        // gltf-rs resolves external .gltf buffer and image references below.
        self.load(relative_path)?;
        let key = (canonical.clone(), mesh_index, primitive_index);
        if let Some(geometry) = self.decoded.get(&key) {
            return Ok(geometry.clone());
        }
        let parsed = gltf::Gltf::open(&canonical)
            .with_context(|| format!("failed to decode glTF asset {}", canonical.display()))?;
        let base = canonical.parent().unwrap_or_else(|| Path::new("."));
        let buffers = gltf::import_buffers(&parsed.document, Some(base), parsed.blob.clone())
            .with_context(|| format!("failed to load glTF buffers {}", canonical.display()))?;
        let images = gltf::import_images(&parsed.document, Some(base), &buffers)
            .with_context(|| format!("failed to load glTF images {}", canonical.display()))?;
        let meshopt_views = decode_meshopt_views(&parsed.document, &buffers)
            .with_context(|| format!("failed to decode meshopt data {}", canonical.display()))?;
        let document = &parsed.document;
        let mesh = document
            .meshes()
            .nth(mesh_index)
            .with_context(|| format!("glTF mesh index out of range: {mesh_index}"))?;
        let primitive = mesh
            .primitives()
            .nth(primitive_index)
            .with_context(|| format!("glTF primitive index out of range: {primitive_index}"))?;
        let positions_accessor = primitive
            .get(&gltf::mesh::Semantic::Positions)
            .context("glTF primitive has no POSITION attribute")?;
        let indices_accessor = primitive.indices();
        let normals_accessor = primitive.get(&gltf::mesh::Semantic::Normals);
        let uvs_accessor = primitive.get(&gltf::mesh::Semantic::TexCoords(0));
        let uses_meshopt = [
            Some(&positions_accessor),
            indices_accessor.as_ref(),
            normals_accessor.as_ref(),
            uvs_accessor.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|accessor| {
            accessor
                .view()
                .is_some_and(|view| meshopt_views.contains_key(&view.index()))
        });
        let reader =
            primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.as_ref()));
        let positions = if uses_meshopt {
            read_vec3_f32(&positions_accessor, &buffers, &meshopt_views)?
        } else {
            reader
                .read_positions()
                .context("glTF primitive has no POSITION attribute")?
                .collect()
        };
        anyhow::ensure!(
            positions.len() >= 3,
            "glTF primitive must contain at least three positions"
        );
        let indices = if uses_meshopt {
            indices_accessor
                .as_ref()
                .map(|accessor| read_indices(accessor, &buffers, &meshopt_views))
                .transpose()?
                .unwrap_or_else(|| (0..positions.len() as u32).collect())
        } else {
            reader
                .read_indices()
                .map(|indices| indices.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect())
        };
        let normals = if uses_meshopt {
            normals_accessor
                .as_ref()
                .map(|accessor| read_vec3_f32(accessor, &buffers, &meshopt_views))
                .transpose()?
                .unwrap_or_else(|| generate_vertex_normals(&positions, &indices))
        } else {
            reader
                .read_normals()
                .map(|normals| normals.collect())
                .unwrap_or_else(|| generate_vertex_normals(&positions, &indices))
        };
        anyhow::ensure!(
            normals.len() == positions.len(),
            "glTF NORMAL count must match POSITION count"
        );
        let uvs = if uses_meshopt {
            uvs_accessor
                .as_ref()
                .map(|accessor| read_vec2_f32(accessor, &buffers, &meshopt_views))
                .transpose()?
                .unwrap_or_default()
        } else {
            reader
                .read_tex_coords(0)
                .map(|coords| coords.into_f32().map(|[u, v]| [u, v]).collect())
                .unwrap_or_default()
        };
        anyhow::ensure!(
            uvs.is_empty() || uvs.len() == positions.len(),
            "glTF TEXCOORD_0 count must match POSITION count"
        );
        let pbr = primitive.material().pbr_metallic_roughness();
        let texture = pbr.base_color_texture().and_then(|texture| {
            let image_index = texture.texture().source().index();
            let image = images.get(image_index)?;
            let rgba8 = image_to_rgba8(image)?;
            let mut texture_hasher = DefaultHasher::new();
            canonical.hash(&mut texture_hasher);
            image_index.hash(&mut texture_hasher);
            Some(AssetTexture {
                texture_id: texture_hasher.finish(),
                width: image.width,
                height: image.height,
                rgba8,
            })
        });
        let material = AssetMaterial {
            base_color: pbr.base_color_factor().map(f64::from),
            metallic: f64::from(pbr.metallic_factor()),
            roughness: f64::from(pbr.roughness_factor()),
            emissive: primitive.material().emissive_factor().map(f64::from),
            ..AssetMaterial::default()
        };
        let mut hasher = DefaultHasher::new();
        canonical.hash(&mut hasher);
        mesh_index.hash(&mut hasher);
        primitive_index.hash(&mut hasher);
        let geometry = Arc::new(AssetGeometry {
            geometry_id: hasher.finish(),
            positions,
            indices,
            normals,
            uvs,
            texture,
            material,
        });
        self.decoded.insert(key, geometry.clone());
        Ok(geometry)
    }

    fn resolve_path(&self, relative: &Path, display_path: &str) -> Result<PathBuf> {
        let path = self.root.join(relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("asset does not exist: {display_path}"))?;
        anyhow::ensure!(
            canonical.starts_with(&self.root),
            "asset path escapes project root: {display_path}"
        );
        Ok(canonical)
    }
}

fn generate_vertex_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let a = positions[triangle[0] as usize];
        let b = positions[triangle[1] as usize];
        let c = positions[triangle[2] as usize];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let face = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        for index in triangle {
            let normal = &mut normals[*index as usize];
            normal[0] += face[0];
            normal[1] += face[1];
            normal[2] += face[2];
        }
    }
    for normal in &mut normals {
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
            .sqrt()
            .max(f32::EPSILON);
        *normal = [normal[0] / length, normal[1] / length, normal[2] / length];
    }
    normals
}

type DecodedMeshoptViews = HashMap<usize, Vec<u8>>;

fn decode_meshopt_views(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<DecodedMeshoptViews> {
    let mut decoded = HashMap::new();
    for view in document.views() {
        let Some(extension) = view.extension_value("EXT_meshopt_compression") else {
            continue;
        };
        let source_buffer = extension_usize(extension, "buffer")?;
        let source_offset = extension_usize_or(extension, "byteOffset", 0);
        let source_length = extension_usize(extension, "byteLength")?;
        let count = extension_usize(extension, "count")?;
        let stride = extension_usize(extension, "byteStride")?;
        let mode = extension_string(extension, "mode")?;
        let source = buffers
            .get(source_buffer)
            .with_context(|| format!("meshopt source buffer out of range: {source_buffer}"))?;
        let source_end = source_offset
            .checked_add(source_length)
            .context("meshopt source range overflow")?;
        let compressed = source
            .get(source_offset..source_end)
            .context("meshopt source range is outside its buffer")?;
        let mut bytes = match mode {
            "ATTRIBUTES" => decode_meshopt_vertices(compressed, count, stride)?,
            "TRIANGLES" => decode_meshopt_indices(compressed, count, stride)?,
            "INDICES" => decode_meshopt_index_sequence(compressed, count, stride)?,
            other => anyhow::bail!("unsupported EXT_meshopt_compression mode: {other}"),
        };
        apply_meshopt_filter(
            &mut bytes,
            stride,
            extension_string_or(extension, "filter", "NONE"),
        )?;
        decoded.insert(view.index(), bytes);
    }
    Ok(decoded)
}

fn decode_meshopt_vertices(compressed: &[u8], count: usize, stride: usize) -> Result<Vec<u8>> {
    macro_rules! decode {
        ($size:literal) => {{
            let mut output = vec![[0_u8; $size]; count];
            meshopt_rs::vertex::buffer::decode_vertex_buffer(&mut output, compressed)
                .map_err(|error| anyhow::anyhow!("meshopt vertex decode failed: {error:?}"))?;
            Ok(output
                .iter()
                .flat_map(|vertex| vertex.iter().copied())
                .collect::<Vec<_>>())
        }};
    }
    match stride {
        4 => decode!(4),
        8 => decode!(8),
        12 => decode!(12),
        16 => decode!(16),
        20 => decode!(20),
        24 => decode!(24),
        28 => decode!(28),
        32 => decode!(32),
        36 => decode!(36),
        40 => decode!(40),
        44 => decode!(44),
        48 => decode!(48),
        52 => decode!(52),
        56 => decode!(56),
        60 => decode!(60),
        64 => decode!(64),
        68 => decode!(68),
        72 => decode!(72),
        76 => decode!(76),
        80 => decode!(80),
        84 => decode!(84),
        88 => decode!(88),
        92 => decode!(92),
        96 => decode!(96),
        100 => decode!(100),
        104 => decode!(104),
        108 => decode!(108),
        112 => decode!(112),
        116 => decode!(116),
        120 => decode!(120),
        124 => decode!(124),
        128 => decode!(128),
        132 => decode!(132),
        136 => decode!(136),
        140 => decode!(140),
        144 => decode!(144),
        148 => decode!(148),
        152 => decode!(152),
        156 => decode!(156),
        160 => decode!(160),
        164 => decode!(164),
        168 => decode!(168),
        172 => decode!(172),
        176 => decode!(176),
        180 => decode!(180),
        184 => decode!(184),
        188 => decode!(188),
        192 => decode!(192),
        196 => decode!(196),
        200 => decode!(200),
        204 => decode!(204),
        208 => decode!(208),
        212 => decode!(212),
        216 => decode!(216),
        220 => decode!(220),
        224 => decode!(224),
        228 => decode!(228),
        232 => decode!(232),
        236 => decode!(236),
        240 => decode!(240),
        244 => decode!(244),
        248 => decode!(248),
        252 => decode!(252),
        256 => decode!(256),
        _ => {
            anyhow::bail!("meshopt attribute stride must be a multiple of four up to 256: {stride}")
        }
    }
}

fn decode_meshopt_indices(compressed: &[u8], count: usize, stride: usize) -> Result<Vec<u8>> {
    anyhow::ensure!(
        stride == 2 || stride == 4,
        "meshopt index stride must be 2 or 4"
    );
    let mut indices = vec![0_u32; count];
    meshopt_rs::index::buffer::decode_index_buffer(&mut indices, compressed)
        .map_err(|error| anyhow::anyhow!("meshopt index decode failed: {error:?}"))?;
    let mut output = Vec::with_capacity(count * stride);
    for index in indices {
        if stride == 2 {
            output.extend_from_slice(&(index as u16).to_le_bytes());
        } else {
            output.extend_from_slice(&index.to_le_bytes());
        }
    }
    Ok(output)
}

fn decode_meshopt_index_sequence(
    compressed: &[u8],
    count: usize,
    stride: usize,
) -> Result<Vec<u8>> {
    anyhow::ensure!(
        stride == 2 || stride == 4,
        "meshopt index stride must be 2 or 4"
    );
    let mut indices = vec![0_u32; count];
    meshopt_rs::index::sequence::decode_index_sequence(&mut indices, compressed)
        .map_err(|error| anyhow::anyhow!("meshopt index sequence decode failed: {error:?}"))?;
    let mut output = Vec::with_capacity(count * stride);
    for index in indices {
        if stride == 2 {
            output.extend_from_slice(&(index as u16).to_le_bytes());
        } else {
            output.extend_from_slice(&index.to_le_bytes());
        }
    }
    Ok(output)
}

fn apply_meshopt_filter(bytes: &mut [u8], stride: usize, filter: &str) -> Result<()> {
    match filter {
        "NONE" => Ok(()),
        "OCTAHEDRAL" if stride == 4 => {
            for chunk in bytes.chunks_exact_mut(4) {
                let mut value = [0_u8; 4];
                value.copy_from_slice(chunk);
                meshopt_rs::vertex::filter::decode_filter_oct_8(std::slice::from_mut(&mut value));
                chunk.copy_from_slice(&value);
            }
            Ok(())
        }
        "OCTAHEDRAL" if stride == 8 => {
            for chunk in bytes.chunks_exact_mut(8) {
                let mut value = [0_u16; 4];
                for (component, raw) in value.iter_mut().zip(chunk.chunks_exact(2)) {
                    *component = u16::from_le_bytes([raw[0], raw[1]]);
                }
                meshopt_rs::vertex::filter::decode_filter_oct_16(std::slice::from_mut(&mut value));
                for (component, raw) in value.iter().zip(chunk.chunks_exact_mut(2)) {
                    raw.copy_from_slice(&component.to_le_bytes());
                }
            }
            Ok(())
        }
        "QUATERNION" if stride == 8 => {
            for chunk in bytes.chunks_exact_mut(8) {
                let mut value = [0_u16; 4];
                for (component, raw) in value.iter_mut().zip(chunk.chunks_exact(2)) {
                    *component = u16::from_le_bytes([raw[0], raw[1]]);
                }
                meshopt_rs::vertex::filter::decode_filter_quat(std::slice::from_mut(&mut value));
                for (component, raw) in value.iter().zip(chunk.chunks_exact_mut(2)) {
                    raw.copy_from_slice(&component.to_le_bytes());
                }
            }
            Ok(())
        }
        "EXPONENTIAL" => {
            anyhow::ensure!(
                bytes.len() % 4 == 0,
                "meshopt exponential filter requires four-byte components"
            );
            let mut values = bytes
                .chunks_exact(4)
                .map(|raw| u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
                .collect::<Vec<_>>();
            meshopt_rs::vertex::filter::decode_filter_exp(&mut values);
            for (value, raw) in values.iter().zip(bytes.chunks_exact_mut(4)) {
                raw.copy_from_slice(&value.to_le_bytes());
            }
            Ok(())
        }
        other => anyhow::bail!("unsupported meshopt filter {other} for stride {stride}"),
    }
}

fn read_vec3_f32(
    accessor: &gltf::Accessor<'_>,
    buffers: &[gltf::buffer::Data],
    decoded: &DecodedMeshoptViews,
) -> Result<Vec<[f32; 3]>> {
    anyhow::ensure!(
        accessor.data_type() == gltf::accessor::DataType::F32
            && accessor.dimensions() == gltf::accessor::Dimensions::Vec3,
        "expected a float VEC3 accessor"
    );
    let data = read_accessor_data(accessor, buffers, decoded)?;
    Ok(data
        .chunks_exact(12)
        .map(|raw| {
            [
                f32::from_le_bytes(raw[0..4].try_into().unwrap()),
                f32::from_le_bytes(raw[4..8].try_into().unwrap()),
                f32::from_le_bytes(raw[8..12].try_into().unwrap()),
            ]
        })
        .collect())
}

fn read_vec2_f32(
    accessor: &gltf::Accessor<'_>,
    buffers: &[gltf::buffer::Data],
    decoded: &DecodedMeshoptViews,
) -> Result<Vec<[f32; 2]>> {
    anyhow::ensure!(
        accessor.dimensions() == gltf::accessor::Dimensions::Vec2,
        "expected a VEC2 texture coordinate accessor"
    );
    let data = read_accessor_data(accessor, buffers, decoded)?;
    let component_size = accessor.data_type().size();
    Ok(data
        .chunks_exact(component_size * 2)
        .map(|raw| {
            [
                read_component_f32(
                    &raw[..component_size],
                    accessor.data_type(),
                    accessor.normalized(),
                ),
                read_component_f32(
                    &raw[component_size..],
                    accessor.data_type(),
                    accessor.normalized(),
                ),
            ]
        })
        .collect())
}

fn read_indices(
    accessor: &gltf::Accessor<'_>,
    buffers: &[gltf::buffer::Data],
    decoded: &DecodedMeshoptViews,
) -> Result<Vec<u32>> {
    anyhow::ensure!(
        accessor.dimensions() == gltf::accessor::Dimensions::Scalar,
        "expected a scalar index accessor"
    );
    let data = read_accessor_data(accessor, buffers, decoded)?;
    Ok(match accessor.data_type() {
        gltf::accessor::DataType::U16 => data
            .chunks_exact(2)
            .map(|raw| u16::from_le_bytes([raw[0], raw[1]]) as u32)
            .collect(),
        gltf::accessor::DataType::U32 => data
            .chunks_exact(4)
            .map(|raw| u32::from_le_bytes(raw.try_into().unwrap()))
            .collect(),
        _ => anyhow::bail!("glTF indices must use UNSIGNED_SHORT or UNSIGNED_INT"),
    })
}

fn read_accessor_data(
    accessor: &gltf::Accessor<'_>,
    buffers: &[gltf::buffer::Data],
    decoded: &DecodedMeshoptViews,
) -> Result<Vec<u8>> {
    let view = accessor
        .view()
        .context("sparse accessors are not supported by the native meshopt path")?;
    let view_data = if let Some(data) = decoded.get(&view.index()) {
        data.as_slice()
    } else {
        let buffer = buffers
            .get(view.buffer().index())
            .context("glTF accessor buffer out of range")?;
        let end = view
            .offset()
            .checked_add(view.length())
            .context("glTF buffer view range overflow")?;
        buffer
            .get(view.offset()..end)
            .context("glTF buffer view range is outside its buffer")?
    };
    let element_size = accessor.size();
    let stride = view.stride().unwrap_or(element_size);
    let mut output = Vec::with_capacity(accessor.count() * element_size);
    for index in 0..accessor.count() {
        let start = accessor
            .offset()
            .checked_add(index * stride)
            .context("glTF accessor offset overflow")?;
        let end = start
            .checked_add(element_size)
            .context("glTF accessor element range overflow")?;
        output.extend_from_slice(
            view_data
                .get(start..end)
                .context("glTF accessor element is outside its buffer view")?,
        );
    }
    Ok(output)
}

fn read_component_f32(raw: &[u8], data_type: gltf::accessor::DataType, normalized: bool) -> f32 {
    match data_type {
        gltf::accessor::DataType::F32 => f32::from_le_bytes(raw.try_into().unwrap()),
        gltf::accessor::DataType::U8 => {
            let value = raw[0] as f32;
            if normalized {
                value / 255.0
            } else {
                value
            }
        }
        gltf::accessor::DataType::U16 => {
            let value = u16::from_le_bytes(raw.try_into().unwrap()) as f32;
            if normalized {
                value / 65535.0
            } else {
                value
            }
        }
        gltf::accessor::DataType::I8 => {
            let value = raw[0] as i8 as f32;
            if normalized {
                (value / 127.0).max(-1.0)
            } else {
                value
            }
        }
        gltf::accessor::DataType::I16 => {
            let value = i16::from_le_bytes(raw.try_into().unwrap()) as f32;
            if normalized {
                (value / 32767.0).max(-1.0)
            } else {
                value
            }
        }
        gltf::accessor::DataType::U32 => u32::from_le_bytes(raw.try_into().unwrap()) as f32,
    }
}

fn extension_usize(value: &Value, key: &str) -> Result<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|number| usize::try_from(number).ok())
        .with_context(|| format!("meshopt extension field {key} is missing or invalid"))
}

fn extension_usize_or(value: &Value, key: &str, default: usize) -> usize {
    extension_usize(value, key).unwrap_or(default)
}

fn extension_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("meshopt extension field {key} is missing or invalid"))
}

fn extension_string_or<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    extension_string(value, key).unwrap_or(default)
}

fn image_to_rgba8(image: &gltf::image::Data) -> Option<Vec<u8>> {
    match image.format {
        gltf::image::Format::R8 => {
            Some(image.pixels.iter().flat_map(|&r| [r, r, r, 255]).collect())
        }
        gltf::image::Format::R8G8 => Some(
            image
                .pixels
                .chunks_exact(2)
                .flat_map(|pixel| [pixel[0], pixel[1], 0, 255])
                .collect(),
        ),
        gltf::image::Format::R8G8B8 => Some(
            image
                .pixels
                .chunks_exact(3)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
        ),
        gltf::image::Format::R8G8B8A8 => Some(image.pixels.clone()),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct InspectedAsset {
    format: String,
    mesh_count: usize,
    primitive_count: usize,
    animation_count: usize,
}

fn validate_relative_path(path: &str) -> Result<PathBuf> {
    anyhow::ensure!(!path.is_empty(), "asset path must not be empty");
    let relative = PathBuf::from(path);
    anyhow::ensure!(!relative.is_absolute(), "asset path must be relative");
    anyhow::ensure!(
        relative
            .components()
            .all(|component| { !matches!(component, std::path::Component::ParentDir) }),
        "asset path must not contain parent traversal"
    );
    Ok(relative)
}

fn inspect_asset(bytes: &[u8], path: &Path) -> Result<InspectedAsset> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_glb = bytes.get(0..4) == Some(b"glTF");
    if extension == "gltf" || extension == "glb" || is_glb {
        let document = gltf::Gltf::from_slice(bytes)
            .with_context(|| format!("failed to parse glTF asset {}", path.display()))?;
        return Ok(InspectedAsset {
            format: if is_glb || extension == "glb" {
                "glb".to_string()
            } else {
                "gltf".to_string()
            },
            mesh_count: document.meshes().count(),
            primitive_count: document
                .meshes()
                .map(|mesh| mesh.primitives().count())
                .sum(),
            animation_count: document.animations().count(),
        });
    }
    Ok(InspectedAsset {
        format: if extension.is_empty() {
            "binary".to_string()
        } else {
            extension
        },
        ..Default::default()
    })
}

impl MappedAsset {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file =
            File::open(path).with_context(|| format!("failed to open asset {}", path.display()))?;
        // The file descriptor remains owned by the OS after mapping. Keeping
        // the mapping read-only prevents accidental mutation of source data.
        let map = unsafe { MmapOptions::new().map(&file) }
            .with_context(|| format!("failed to mmap asset {}", path.display()))?;
        Ok(Self { map })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.map
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::AssetStore;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn loads_project_relative_binary_and_reports_metadata() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-asset-test-{suffix}"));
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(root.join("public/test.bin"), [1_u8, 2, 3, 4]).unwrap();
        fs::write(
            root.join("public/scene.gltf"),
            br#"{
              "asset": {"version": "2.0"},
              "buffers": [{"byteLength": 36, "uri": "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}],
              "bufferViews": [{"buffer": 0, "byteLength": 36}, {"buffer": 0, "byteLength": 24}],
              "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 1]}, {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"}],
              "images": [{"uri": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="}],
              "textures": [{"source": 0}],
              "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
              "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 1}, "material": 0}]}],
              "animations": [{"channels": [], "samplers": []}]
            }"#,
        )
        .unwrap();
        let mut store = AssetStore::new(&root).unwrap();
        let metadata = store.load("public/test.bin").unwrap();
        assert_eq!(metadata.byte_length, 4);
        assert_eq!(metadata.format, "bin");
        let gltf = store.load("public/scene.gltf").unwrap();
        assert_eq!(gltf.format, "gltf");
        assert_eq!(gltf.mesh_count, 1);
        assert_eq!(gltf.primitive_count, 1);
        assert_eq!(gltf.animation_count, 1);
        let geometry = store.load_geometry("public/scene.gltf", 0, 0).unwrap();
        assert_eq!(geometry.positions.len(), 3);
        assert_eq!(geometry.indices, [0, 1, 2]);
        assert_eq!(geometry.normals.len(), 3);
        assert_eq!(geometry.uvs.len(), 3);
        let texture = geometry.texture.as_ref().unwrap();
        assert_eq!((texture.width, texture.height), (1, 1));
        assert_eq!(texture.rgba8.len(), 4);
        assert_eq!(geometry.material.roughness, 1.0);
        assert!(store.load("../outside.bin").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn decodes_meshopt_vertex_and_index_streams() {
        let vertices = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mut encoded_vertices = vec![
            0_u8;
            meshopt_rs::vertex::buffer::encode_vertex_buffer_bound(
                vertices.len(),
                std::mem::size_of::<[f32; 3]>(),
            )
        ];
        let encoded_vertex_len = meshopt_rs::vertex::buffer::encode_vertex_buffer(
            &mut encoded_vertices,
            &vertices,
            meshopt_rs::vertex::VertexEncodingVersion::default(),
        )
        .unwrap();
        encoded_vertices.truncate(encoded_vertex_len);
        let decoded_vertices =
            super::decode_meshopt_vertices(&encoded_vertices, vertices.len(), 12).unwrap();
        assert_eq!(decoded_vertices.len(), std::mem::size_of_val(&vertices));
        for (actual, expected) in decoded_vertices
            .chunks_exact(4)
            .zip(vertices.iter().flat_map(|vertex| vertex.iter()))
        {
            assert_eq!(f32::from_le_bytes(actual.try_into().unwrap()), *expected);
        }

        let indices = [0_u32, 1, 2];
        let mut encoded_indices =
            vec![
                0_u8;
                meshopt_rs::index::buffer::encode_index_buffer_bound(indices.len(), vertices.len(),)
            ];
        let encoded_index_len = meshopt_rs::index::buffer::encode_index_buffer(
            &mut encoded_indices,
            &indices,
            meshopt_rs::index::IndexEncodingVersion::default(),
        )
        .unwrap();
        encoded_indices.truncate(encoded_index_len);
        let decoded_indices =
            super::decode_meshopt_indices(&encoded_indices, indices.len(), 2).unwrap();
        assert_eq!(decoded_indices, [0, 0, 1, 0, 2, 0]);

        let sequence = [0_u32, 1, 7, 2, 6, 9];
        let mut encoded_sequence = vec![
            0_u8;
            meshopt_rs::index::sequence::encode_index_sequence_bound(
                sequence.len(),
                vertices.len(),
            )
        ];
        let encoded_sequence_len = meshopt_rs::index::sequence::encode_index_sequence(
            &mut encoded_sequence,
            &sequence,
            meshopt_rs::index::IndexEncodingVersion::default(),
        );
        encoded_sequence.truncate(encoded_sequence_len);
        let decoded_sequence =
            super::decode_meshopt_index_sequence(&encoded_sequence, sequence.len(), 4).unwrap();
        assert_eq!(
            decoded_sequence,
            [0, 0, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0, 2, 0, 0, 0, 6, 0, 0, 0, 9, 0, 0, 0]
        );
    }
}
