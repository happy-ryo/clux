use anyhow::Result;
use tracing::info;

use crate::atlas::GlyphAtlas;
use crate::border::BorderRenderer;
use crate::cell_renderer::{CellInstance, CellRenderer};

/// Default font size in pixels.
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// GPU rendering pipeline state.
pub struct RenderPipeline {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    cell_renderer: CellRenderer,
    glyph_atlas: GlyphAtlas,
    border_renderer: BorderRenderer,
}

impl RenderPipeline {
    pub async fn new(window: std::sync::Arc<winit::window::Window>) -> Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        info!(
            adapter = adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU adapter selected"
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("clux device"),
                ..Default::default()
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Initialize cell renderer and glyph atlas
        let mut cell_renderer = CellRenderer::new(&device, surface_format);
        let glyph_atlas = GlyphAtlas::new(&device, &queue, DEFAULT_FONT_SIZE);

        // Connect atlas texture to the cell renderer
        cell_renderer.set_atlas_texture(&device, &glyph_atlas.view);

        // Update uniforms with initial screen size
        cell_renderer.update_uniforms(&queue, config.width as f32, config.height as f32);

        let border_renderer = BorderRenderer::new();

        info!("Cell renderer and glyph atlas initialized");

        Ok(Self {
            surface,
            device,
            queue,
            config,
            cell_renderer,
            glyph_atlas,
            border_renderer,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.cell_renderer
                .update_uniforms(&self.queue, width as f32, height as f32);
        }
    }

    /// Access the glyph atlas for looking up / inserting glyphs.
    pub fn glyph_atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.glyph_atlas
    }

    /// The default font size used by this pipeline.
    pub fn font_size(&self) -> f32 {
        DEFAULT_FONT_SIZE
    }

    /// Measure actual cell dimensions from the font.
    /// Returns `(cell_width, cell_height)` in logical pixels.
    pub fn measure_cell_size(&mut self) -> (f32, f32) {
        self.glyph_atlas.measure_cell_size(DEFAULT_FONT_SIZE)
    }

    /// Access the device (needed for atlas operations from app code).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Access the queue (needed for atlas operations from app code).
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Look up or insert a glyph in the atlas, returning its info.
    /// This avoids borrow conflicts by accessing atlas and queue together.
    pub fn get_or_insert_glyph(
        &mut self,
        c: char,
        font_size: f32,
    ) -> Option<crate::atlas::GlyphInfo> {
        self.glyph_atlas.get_or_insert(c, font_size, &self.queue)
    }

    pub fn render_frame(&mut self, clear_color: wgpu::Color, cells: &[CellInstance]) -> Result<()> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        // Upload cell instance data
        self.cell_renderer.prepare(&self.queue, cells);

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // Render cells (backgrounds first, then foreground glyphs - sorted by caller)
            self.cell_renderer
                .render(&mut render_pass, cells.len() as u32);

            // Render pane borders (stub)
            self.border_renderer.render(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
