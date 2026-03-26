use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent};
use tracing::debug;

/// Information about a rasterized glyph stored in the atlas.
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// U coordinate in atlas (normalized 0..1).
    pub u: f32,
    /// V coordinate in atlas (normalized 0..1).
    pub v: f32,
    /// Width in atlas (normalized 0..1).
    pub uv_w: f32,
    /// Height in atlas (normalized 0..1).
    pub uv_h: f32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Horizontal offset from the origin.
    pub offset_x: i32,
    /// Vertical offset from the origin.
    pub offset_y: i32,
}

/// Default monospace font family for terminal rendering.
const FONT_FAMILY: &str = "Consolas";

/// Create font attributes with the configured monospace font.
fn mono_attrs() -> Attrs<'static> {
    Attrs::new().family(Family::Name(FONT_FAMILY))
}

/// Row-based packing state for the glyph atlas.
struct PackingState {
    current_x: u32,
    current_y: u32,
    row_height: u32,
}

/// A texture atlas that stores rasterized glyphs for GPU rendering.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    font_system: FontSystem,
    swash_cache: SwashCache,
    cache: HashMap<(char, u32), GlyphInfo>,
    packing: PackingState,
}

impl GlyphAtlas {
    /// Default atlas size (1024x1024 single-channel).
    const ATLAS_SIZE: u32 = 1024;

    /// Create a new glyph atlas and pre-populate ASCII characters.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, font_size: f32) -> Self {
        let width = Self::ATLAS_SIZE;
        let height = Self::ATLAS_SIZE;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        let mut atlas = Self {
            texture,
            view,
            width,
            height,
            font_system,
            swash_cache,
            cache: HashMap::new(),
            packing: PackingState {
                current_x: 0,
                current_y: 0,
                row_height: 0,
            },
        };

        // Pre-populate printable ASCII (32..=126)
        atlas.prepopulate_ascii(queue, font_size);

        atlas
    }

    /// Measure the cell dimensions for a monospace grid based on actual font metrics.
    /// Returns `(cell_width, cell_height)` in logical pixels.
    pub fn measure_cell_size(&mut self, font_size: f32) -> (f32, f32) {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let attrs = mono_attrs();

        // Measure a representative character ('M' is typically full-width in monospace)
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(&mut self.font_system, "M", attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut cell_width = font_size * 0.6; // fallback
        let cell_height = metrics.line_height;

        for run in buffer.layout_runs() {
            if let Some(glyph) = run.glyphs.first() {
                cell_width = glyph.w;
                break;
            }
        }

        debug!(
            cell_width,
            cell_height, font_size, "Measured cell size from font metrics"
        );
        (cell_width, cell_height)
    }

    fn prepopulate_ascii(&mut self, queue: &wgpu::Queue, font_size: f32) {
        let mut count = 0u32;
        for c in ' '..='~' {
            if self.get_or_insert(c, font_size, queue).is_some() {
                count += 1;
            }
        }
        debug!(count, "Pre-populated ASCII glyphs in atlas");
    }

    /// Look up or rasterize a glyph, returning its atlas info.
    pub fn get_or_insert(
        &mut self,
        c: char,
        font_size: f32,
        queue: &wgpu::Queue,
    ) -> Option<GlyphInfo> {
        let key = (c, font_size.to_bits());
        if let Some(&info) = self.cache.get(&key) {
            return Some(info);
        }

        // Rasterize the glyph using cosmic-text
        let (image_data, glyph_w, glyph_h, offset_x, offset_y) = self.rasterize_char(c, font_size);

        if glyph_w == 0 || glyph_h == 0 {
            // Whitespace or empty glyph - store a zero-size entry
            let info = GlyphInfo {
                u: 0.0,
                v: 0.0,
                uv_w: 0.0,
                uv_h: 0.0,
                width: 0,
                height: 0,
                offset_x,
                offset_y,
            };
            self.cache.insert(key, info);
            return Some(info);
        }

        // Pack into atlas using row-based packing
        let (atlas_x, atlas_y) = self.allocate(glyph_w, glyph_h)?;

        // Upload to GPU
        self.upload_region(queue, &image_data, atlas_x, atlas_y, glyph_w, glyph_h);

        let info = GlyphInfo {
            u: atlas_x as f32 / self.width as f32,
            v: atlas_y as f32 / self.height as f32,
            uv_w: glyph_w as f32 / self.width as f32,
            uv_h: glyph_h as f32 / self.height as f32,
            width: glyph_w,
            height: glyph_h,
            offset_x,
            offset_y,
        };

        self.cache.insert(key, info);
        Some(info)
    }

    /// Rasterize a character, returning `(pixel_data, width, height, offset_x, offset_y)`.
    fn rasterize_char(&mut self, c: char, font_size: f32) -> (Vec<u8>, u32, u32, i32, i32) {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let attrs = mono_attrs();

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(
            &mut self.font_system,
            &c.to_string(),
            attrs,
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Find the first physical glyph in the buffer
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, 0.0), 1.0);

                if let Some(image) = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical.cache_key)
                {
                    let w = image.placement.width;
                    let h = image.placement.height;

                    let data = match image.content {
                        SwashContent::Mask => image.data.clone(),
                        SwashContent::Color => {
                            // Convert RGBA to single-channel alpha
                            image.data.chunks(4).map(|px| px[3]).collect()
                        }
                        SwashContent::SubpixelMask => {
                            // Average the RGB subpixel channels
                            image
                                .data
                                .chunks(3)
                                .map(|px| {
                                    let sum =
                                        u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2]);
                                    (sum / 3) as u8
                                })
                                .collect()
                        }
                    };

                    return (data, w, h, image.placement.left, image.placement.top);
                }
            }
        }

        // No glyph found - empty entry for spaces etc.
        (vec![], 0, 0, 0, 0)
    }

    /// Allocate a rectangle in the atlas using simple row packing.
    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        // Add 1px padding between glyphs
        let padded_w = w + 1;
        let padded_h = h + 1;

        if self.packing.current_x + padded_w > self.width {
            // Move to next row
            self.packing.current_y += self.packing.row_height;
            self.packing.current_x = 0;
            self.packing.row_height = 0;
        }

        if self.packing.current_y + padded_h > self.height {
            tracing::warn!("Glyph atlas full");
            return None;
        }

        let x = self.packing.current_x;
        let y = self.packing.current_y;

        self.packing.current_x += padded_w;
        self.packing.row_height = self.packing.row_height.max(padded_h);

        Some((x, y))
    }

    /// Upload pixel data to a region of the atlas texture.
    fn upload_region(&self, queue: &wgpu::Queue, data: &[u8], x: u32, y: u32, w: u32, h: u32) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
}
