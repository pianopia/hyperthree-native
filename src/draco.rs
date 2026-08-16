use anyhow::{bail, Context, Result};
use draco_oxide_core::{
    attribute::{Attribute, AttributeType, ComponentDataType},
    types::{DataValue, NdVector, PointIdx, Vector},
};
use draco_oxide_decoder::{Decoder, Geometry};

#[derive(Debug, Clone)]
pub struct DracoAttribute {
    pub name: String,
    pub item_size: usize,
    pub data: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct DracoGeometry {
    pub indices: Vec<u32>,
    pub attributes: Vec<DracoAttribute>,
    pub point_cloud: bool,
}

/// Decode either a triangular mesh or a Draco point-cloud stream. Point clouds
/// use a sequential index buffer so the JS standalone boundary can feed the
/// same attribute representation to Three.js `Points`/BufferGeometry code.
pub fn decode_geometry(bytes: &[u8]) -> Result<DracoGeometry> {
    let geometry = Decoder::new()
        .decode(bytes)
        .map_err(|error| anyhow::anyhow!("Draco geometry decode failed: {error}"))?;
    match geometry {
        Geometry::Mesh(mesh) => {
            let mut indices = Vec::with_capacity(mesh.faces.len() * 3);
            for face in &mesh.faces {
                for point in face {
                    indices.push(
                        u32::try_from(usize::from(*point)).context("Draco index exceeds u32")?,
                    );
                }
            }
            let attributes = decode_attributes(&mesh.attributes)?;
            ensure_position(&attributes)?;
            Ok(DracoGeometry {
                indices,
                attributes,
                point_cloud: false,
            })
        }
        Geometry::PointCloud(point_cloud) => {
            let point_count = point_cloud.num_points();
            let indices = (0..point_count)
                .map(|index| u32::try_from(index).context("Draco point index exceeds u32"))
                .collect::<Result<Vec<_>>>()?;
            let attributes = decode_attributes(point_cloud.attributes())?;
            ensure_position(&attributes)?;
            Ok(DracoGeometry {
                indices,
                attributes,
                point_cloud: true,
            })
        }
        _ => bail!("unsupported Draco geometry kind"),
    }
}

fn decode_attributes(attributes: &[Attribute]) -> Result<Vec<DracoAttribute>> {
    let mut decoded_attributes = Vec::new();
    let mut texcoord_index = 0;
    let mut color_index = 0;
    for attribute in attributes {
        let Some(name) = semantic_name(
            attribute.get_attribute_type(),
            &mut texcoord_index,
            &mut color_index,
        ) else {
            continue;
        };
        let item_size = attribute.get_num_components();
        anyhow::ensure!(
            (1..=4).contains(&item_size),
            "unsupported Draco attribute width {item_size}"
        );
        let data = decode_attribute_values(attribute, item_size)
            .with_context(|| format!("failed to decode Draco attribute {name}"))?;
        decoded_attributes.push(DracoAttribute {
            name: name.to_string(),
            item_size,
            data,
        });
    }
    Ok(decoded_attributes)
}

fn ensure_position(attributes: &[DracoAttribute]) -> Result<()> {
    if !attributes
        .iter()
        .any(|attribute| attribute.name == "position")
    {
        bail!("Draco stream has no POSITION attribute");
    }
    Ok(())
}

fn semantic_name(
    attribute_type: AttributeType,
    texcoord_index: &mut usize,
    color_index: &mut usize,
) -> Option<&'static str> {
    match attribute_type {
        AttributeType::Position => Some("position"),
        AttributeType::Normal => Some("normal"),
        AttributeType::Tangent => Some("tangent"),
        AttributeType::TextureCoordinate => {
            let name = match *texcoord_index {
                0 => "uv",
                1 => "uv1",
                _ => return None,
            };
            *texcoord_index += 1;
            Some(name)
        }
        AttributeType::Color => {
            let name = match *color_index {
                0 => "color",
                1 => "color1",
                _ => return None,
            };
            *color_index += 1;
            Some(name)
        }
        AttributeType::Joint => Some("skinIndex"),
        AttributeType::Weight => Some("skinWeight"),
        AttributeType::Material | AttributeType::Custom | AttributeType::Invalid => None,
    }
}

fn decode_attribute_values(attribute: &Attribute, item_size: usize) -> Result<Vec<f32>> {
    let mut output = Vec::with_capacity(attribute.len() * item_size);
    macro_rules! append_for_type {
        ($type:ty) => {
            match item_size {
                1 => append_values::<$type, 1>(attribute, &mut output),
                2 => append_values::<$type, 2>(attribute, &mut output),
                3 => append_values::<$type, 3>(attribute, &mut output),
                4 => append_values::<$type, 4>(attribute, &mut output),
                _ => unreachable!("validated above"),
            }
        };
    }
    match attribute.get_component_type() {
        ComponentDataType::I8 => append_for_type!(i8),
        ComponentDataType::U8 => append_for_type!(u8),
        ComponentDataType::I16 => append_for_type!(i16),
        ComponentDataType::U16 => append_for_type!(u16),
        ComponentDataType::I32 => append_for_type!(i32),
        ComponentDataType::U32 => append_for_type!(u32),
        ComponentDataType::I64 => append_for_type!(i64),
        ComponentDataType::U64 => append_for_type!(u64),
        ComponentDataType::F32 => append_for_type!(f32),
        ComponentDataType::F64 => append_for_type!(f64),
        ComponentDataType::Invalid => bail!("Draco attribute has invalid component type"),
    }
    Ok(output)
}

fn append_values<T, const N: usize>(attribute: &Attribute, output: &mut Vec<f32>)
where
    T: DataValue,
    NdVector<N, T>: Vector<N, Component = T>,
{
    for index in 0..attribute.len() {
        let value: NdVector<N, T> = attribute.get(PointIdx::from(index));
        for component in 0..N {
            output.push(value.get(component).to_f64() as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::decode_geometry;

    #[test]
    fn decodes_khronos_box_draco_stream() {
        let bytes = [
            0x44, 0x52, 0x41, 0x43, 0x4f, 0x02, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x08, 0x0c,
            0x01, 0x0b, 0x00, 0x00, 0x03, 0x5f, 0x5b, 0x0a, 0x01, 0x01, 0x10, 0x55, 0x04, 0x5c,
            0xe3, 0x8d, 0x46, 0x02, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x09, 0x03,
            0x00, 0x01, 0x02, 0x01, 0x01, 0x09, 0x03, 0x00, 0x00, 0x03, 0x01, 0x01, 0x01, 0x00,
            0x03, 0x03, 0x01, 0x30, 0x01, 0x10, 0x03, 0x00, 0x24, 0x96, 0x13, 0x0a, 0x24, 0x04,
            0x00, 0x00, 0x00, 0x00, 0xff, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x00,
            0x00, 0xbf, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x00, 0x80, 0x3f, 0x0b, 0x06, 0x03, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x40, 0x01, 0x00, 0xff, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00,
            0x00, 0xff, 0x02, 0xa1, 0x41, 0x08, 0x00, 0x00,
        ];
        let geometry = decode_geometry(&bytes).expect("Khronos Box Draco stream should decode");
        assert_eq!(geometry.indices.len(), 36);
        assert_eq!(geometry.attributes.len(), 2);
        assert_eq!(geometry.attributes[0].item_size, 3);
        assert!(geometry
            .attributes
            .iter()
            .any(|attribute| attribute.name == "position"));
        assert!(geometry
            .attributes
            .iter()
            .any(|attribute| attribute.name == "normal"));
    }

    #[test]
    fn geometry_dispatcher_preserves_mesh_shape_for_standard_loader() {
        let bytes = [
            0x44, 0x52, 0x41, 0x43, 0x4f, 0x02, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x08, 0x0c,
            0x01, 0x0b, 0x00, 0x00, 0x03, 0x5f, 0x5b, 0x0a, 0x01, 0x01, 0x10, 0x55, 0x04, 0x5c,
            0xe3, 0x8d, 0x46, 0x02, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x09, 0x03,
            0x00, 0x01, 0x02, 0x01, 0x01, 0x09, 0x03, 0x00, 0x00, 0x03, 0x01, 0x01, 0x01, 0x00,
            0x03, 0x03, 0x01, 0x30, 0x01, 0x10, 0x03, 0x00, 0x24, 0x96, 0x13, 0x0a, 0x24, 0x04,
            0x00, 0x00, 0x00, 0x00, 0xff, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x00,
            0x00, 0xbf, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x00, 0x80, 0x3f, 0x0b, 0x06, 0x03, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x40, 0x01, 0x00, 0xff, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00,
            0x00, 0xff, 0x02, 0xa1, 0x41, 0x08, 0x00, 0x00,
        ];
        let geometry = decode_geometry(&bytes).expect("Draco mesh should dispatch as geometry");
        assert!(!geometry.point_cloud);
        assert_eq!(geometry.indices.len(), 36);
        assert!(geometry
            .attributes
            .iter()
            .any(|attribute| attribute.name == "position"));
    }
}
