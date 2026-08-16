struct Instance {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0)
var<storage, read> instances: array<Instance>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let instance = instances[instance_index];
    var output: VertexOutput;
    output.position = instance.mvp * vec4<f32>(position, 1.0);
    output.color = instance.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
