use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// A single cell instance to be rendered as a quad on the GPU.
///
/// Mode: 0.0 = background fill (solid color), 1.0 = foreground glyph (textured).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CellInstance {
    pub pos_x: f32,
    pub pos_y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
    pub color_a: f32,
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
    pub mode: f32,
    /// Padding to align to 16 bytes.
    pad0: f32,
    pad1: f32,
    pad2: f32,
}

impl CellInstance {
    /// Create a background fill instance (solid color quad).
    #[allow(clippy::many_single_char_names)]
    pub fn background(
        pos_x: f32,
        pos_y: f32,
        size_x: f32,
        size_y: f32,
        red: f32,
        green: f32,
        blue: f32,
    ) -> Self {
        Self {
            pos_x,
            pos_y,
            size_x,
            size_y,
            color_r: red,
            color_g: green,
            color_b: blue,
            color_a: 1.0,
            uv_x: 0.0,
            uv_y: 0.0,
            uv_w: 0.0,
            uv_h: 0.0,
            mode: 0.0,
            pad0: 0.0,
            pad1: 0.0,
            pad2: 0.0,
        }
    }

    /// Create a foreground glyph instance (textured quad from atlas).
    #[allow(clippy::too_many_arguments)]
    pub fn glyph(
        pos_x: f32,
        pos_y: f32,
        size_x: f32,
        size_y: f32,
        red: f32,
        green: f32,
        blue: f32,
        uv_x: f32,
        uv_y: f32,
        uv_w: f32,
        uv_h: f32,
    ) -> Self {
        Self {
            pos_x,
            pos_y,
            size_x,
            size_y,
            color_r: red,
            color_g: green,
            color_b: blue,
            color_a: 1.0,
            uv_x,
            uv_y,
            uv_w,
            uv_h,
            mode: 1.0,
            pad0: 0.0,
            pad1: 0.0,
            pad2: 0.0,
        }
    }
}

/// Uniforms sent to the GPU: screen dimensions for NDC conversion.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub screen_width: f32,
    pub screen_height: f32,
    pad0: f32,
    pad1: f32,
}

/// Maximum number of cell instances per frame.
const MAX_INSTANCES: u64 = 65_536;

/// Renders terminal cells as instanced quads.
pub struct CellRenderer {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
}

/// Unit quad vertices (two triangles forming a quad from (0,0) to (1,1)).
const QUAD_VERTICES: &[[f32; 2]] = &[
    [0.0, 0.0], // top-left
    [1.0, 0.0], // top-right
    [1.0, 1.0], // bottom-right
    [0.0, 1.0], // bottom-left
];

const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

/// Instance buffer vertex attributes for the cell shader.
fn instance_attributes() -> Vec<wgpu::VertexAttribute> {
    vec![
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 8,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: 3,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 32,
            shader_location: 4,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 48,
            shader_location: 5,
            format: wgpu::VertexFormat::Float32,
        },
    ]
}

/// Create the render pipeline for cell rendering.
fn create_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    uniform_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader_source = include_str!("../../../resources/shaders/cell.wgsl");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cell shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cell pipeline layout"),
        bind_group_layouts: &[uniform_layout, texture_layout],
        push_constant_ranges: &[],
    });

    let instance_attrs = instance_attributes();

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cell render pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CellInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instance_attrs,
                },
            ],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

impl CellRenderer {
    /// Create a new cell renderer with its pipeline and buffers.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("uniform bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let render_pipeline = create_pipeline(
            device,
            surface_format,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        let (vertex_buffer, index_buffer, instance_buffer, uniform_buffer, uniform_bind_group) =
            Self::create_buffers(device, &uniform_bind_group_layout);

        Self {
            render_pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_layout,
            texture_bind_group_layout,
            texture_bind_group: None,
        }
    }

    /// Create all GPU buffers and the uniform bind group.
    fn create_buffers(
        device: &wgpu::Device,
        uniform_layout: &wgpu::BindGroupLayout,
    ) -> (
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::BindGroup,
    ) {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell vertex buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell index buffer"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell instance buffer"),
            size: MAX_INSTANCES * std::mem::size_of::<CellInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniforms = Uniforms {
            screen_width: 800.0,
            screen_height: 600.0,
            pad0: 0.0,
            pad1: 0.0,
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell uniform buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bind group"),
            layout: uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        (
            vertex_buffer,
            index_buffer,
            instance_buffer,
            uniform_buffer,
            uniform_bind_group,
        )
    }

    /// Set up the texture bind group from a glyph atlas texture view.
    pub fn set_atlas_texture(&mut self, device: &wgpu::Device, atlas_view: &wgpu::TextureView) {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        self.texture_bind_group = Some(bind_group);
    }

    /// Update the screen dimensions uniform.
    pub fn update_uniforms(&self, queue: &wgpu::Queue, width: f32, height: f32) {
        let uniforms = Uniforms {
            screen_width: width,
            screen_height: height,
            pad0: 0.0,
            pad1: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Upload instance data to the GPU buffer.
    pub fn prepare(&self, queue: &wgpu::Queue, cells: &[CellInstance]) {
        if cells.is_empty() {
            return;
        }
        let data = bytemuck::cast_slice(cells);
        queue.write_buffer(&self.instance_buffer, 0, data);
    }

    /// Issue the draw call for the given number of cell instances.
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, instance_count: u32) {
        if instance_count == 0 {
            return;
        }

        let Some(ref texture_bind_group) = self.texture_bind_group else {
            return;
        };

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..instance_count);
    }
}
