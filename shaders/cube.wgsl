struct Instance {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    base_color: vec4<f32>,
    material: vec4<f32>,
    emissive: vec4<f32>,
};

struct FrameUniform {
    camera_position: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
};

@group(0) @binding(0)
var<storage, read> instances: array<Instance>;

@group(0) @binding(1)
var diffuse_texture: texture_2d<f32>;

@group(0) @binding(2)
var diffuse_sampler: sampler;

@group(0) @binding(3)
var<uniform> frame: FrameUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) base_color: vec4<f32>,
    @location(4) material: vec4<f32>,
    @location(5) emissive: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let instance = instances[instance_index];
    let world = instance.model * vec4<f32>(position, 1.0);
    var output: VertexOutput;
    output.position = instance.mvp * vec4<f32>(position, 1.0);
    output.world_position = world.xyz;
    output.world_normal = normalize((instance.normal_matrix * vec4<f32>(normal, 0.0)).xyz);
    output.uv = uv;
    output.base_color = instance.base_color;
    output.material = instance.material;
    output.emissive = instance.emissive;
    return output;
}

fn fresnel_schlick(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cosine, 5.0);
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denominator = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denominator * denominator, 0.0001);
}

fn geometry_schlick_ggx(n_dot_x: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_x / max(n_dot_x * (1.0 - k) + k, 0.0001);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(diffuse_texture, diffuse_sampler, input.uv);
    let base = sampled * input.base_color;
    if input.material.z > 0.5 {
        return vec4<f32>(base.rgb + input.emissive.rgb, base.a);
    }

    let n = normalize(input.world_normal);
    let v = normalize(frame.camera_position.xyz - input.world_position);
    let l = normalize(frame.light_direction.xyz);
    let h = normalize(v + l);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);
    let metallic = clamp(input.material.x, 0.0, 1.0);
    let roughness = clamp(input.material.y, 0.045, 1.0);
    let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
    let f = fresnel_schlick(v_dot_h, f0);
    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 0.0001);
    let diffuse = (vec3<f32>(1.0) - f) * (1.0 - metallic) * base.rgb / 3.14159265;
    let direct = (diffuse + specular) * frame.light_color.rgb * frame.light_direction.w * n_dot_l;
    let ambient = frame.ambient.rgb * base.rgb * (1.0 - metallic);
    return vec4<f32>(ambient + direct + input.emissive.rgb, base.a);
}
