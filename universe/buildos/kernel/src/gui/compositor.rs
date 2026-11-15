//! BuildOS Window Compositor
//!
//! Handles window compositing, alpha blending, and effects.

#![allow(dead_code)]

use super::{Rect, Point, Color};
use crate::drivers::graphics::Framebuffer;
use crate::math::F32Ext;
use alloc::vec::Vec;
use alloc::boxed::Box;

/// Composition mode for blending windows
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionMode {
    /// Source overwrites destination
    SourceOver,
    /// Multiply blend
    Multiply,
    /// Screen blend
    Screen,
    /// Additive blend
    Additive,
    /// Source only where destination exists
    SourceAtop,
    /// Destination only where source exists
    DestinationOver,
    /// XOR blend
    Xor,
}

/// Visual effect type
#[derive(Clone, Copy, Debug)]
pub enum Effect {
    /// No effect
    None,
    /// Drop shadow
    Shadow {
        offset_x: i32,
        offset_y: i32,
        blur_radius: u32,
        color: Color,
    },
    /// Blur effect
    Blur {
        radius: u32,
    },
    /// Rounded corners
    RoundedCorners {
        radius: u32,
    },
    /// Glass/translucent effect
    Glass {
        tint: Color,
        blur: u32,
    },
    /// Fade in/out
    Fade {
        opacity: u8,
    },
}

/// Layer in the composition stack
pub struct CompositorLayer {
    /// Layer ID
    pub id: u32,
    /// Source buffer
    pub buffer: Vec<u8>,
    /// Buffer dimensions
    pub width: u32,
    pub height: u32,
    /// Position on screen
    pub position: Point,
    /// Layer opacity (0-255)
    pub opacity: u8,
    /// Composition mode
    pub mode: CompositionMode,
    /// Effects to apply
    pub effects: Vec<Effect>,
    /// Is visible
    pub visible: bool,
    /// Z-order (higher = front)
    pub z_order: i32,
}

impl CompositorLayer {
    /// Create a new layer
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        let buffer_size = (width * height * 4) as usize;
        Self {
            id,
            buffer: alloc::vec![0u8; buffer_size],
            width,
            height,
            position: Point::zero(),
            opacity: 255,
            mode: CompositionMode::SourceOver,
            effects: Vec::new(),
            visible: true,
            z_order: 0,
        }
    }

    /// Get pixel at position
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.width || y >= self.height {
            return Color::TRANSPARENT;
        }

        let offset = ((y * self.width + x) * 4) as usize;
        if offset + 3 < self.buffer.len() {
            Color {
                b: self.buffer[offset],
                g: self.buffer[offset + 1],
                r: self.buffer[offset + 2],
                a: self.buffer[offset + 3],
            }
        } else {
            Color::TRANSPARENT
        }
    }

    /// Set pixel at position
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = ((y * self.width + x) * 4) as usize;
        if offset + 3 < self.buffer.len() {
            self.buffer[offset] = color.b;
            self.buffer[offset + 1] = color.g;
            self.buffer[offset + 2] = color.r;
            self.buffer[offset + 3] = color.a;
        }
    }

    /// Clear layer with color
    pub fn clear(&mut self, color: Color) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Get layer bounds
    pub fn bounds(&self) -> Rect {
        Rect::new(self.position.x, self.position.y, self.width, self.height)
    }
}

/// Window compositor
pub struct Compositor {
    /// Output framebuffer
    output: Option<Box<Framebuffer>>,
    /// Composition buffer (back buffer)
    compose_buffer: Vec<u8>,
    /// Buffer dimensions
    width: u32,
    height: u32,
    /// Layers
    layers: Vec<CompositorLayer>,
    /// Background color
    background: Color,
    /// Enable vertical sync
    vsync: bool,
    /// Enable double buffering
    double_buffer: bool,
    /// Dirty regions that need recomposition
    dirty_regions: Vec<Rect>,
}

impl Compositor {
    /// Create a new compositor
    pub fn new(width: u32, height: u32) -> Self {
        let buffer_size = (width * height * 4) as usize;
        Self {
            output: None,
            compose_buffer: alloc::vec![0u8; buffer_size],
            width,
            height,
            layers: Vec::new(),
            background: Color::rgb(32, 32, 64),
            vsync: true,
            double_buffer: true,
            dirty_regions: Vec::new(),
        }
    }

    /// Set output framebuffer
    pub fn set_output(&mut self, fb: Framebuffer) {
        self.output = Some(Box::new(fb));
    }

    /// Add a layer
    pub fn add_layer(&mut self, layer: CompositorLayer) -> u32 {
        let id = layer.id;
        self.layers.push(layer);
        self.sort_layers();
        id
    }

    /// Remove a layer
    pub fn remove_layer(&mut self, id: u32) -> Option<CompositorLayer> {
        if let Some(idx) = self.layers.iter().position(|l| l.id == id) {
            Some(self.layers.remove(idx))
        } else {
            None
        }
    }

    /// Get layer by ID
    pub fn get_layer(&self, id: u32) -> Option<&CompositorLayer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Get mutable layer by ID
    pub fn get_layer_mut(&mut self, id: u32) -> Option<&mut CompositorLayer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Sort layers by z-order
    fn sort_layers(&mut self) {
        self.layers.sort_by_key(|l| l.z_order);
    }

    /// Mark region as dirty
    pub fn mark_dirty(&mut self, rect: Rect) {
        // Merge with existing dirty regions
        for existing in &mut self.dirty_regions {
            if existing.intersects(&rect) {
                *existing = existing.union(&rect);
                return;
            }
        }
        self.dirty_regions.push(rect);
    }

    /// Compose all layers
    pub fn compose(&mut self) {
        if self.dirty_regions.is_empty() {
            return;
        }

        // Compose each dirty region
        for rect in &self.dirty_regions.clone() {
            self.compose_region(rect);
        }

        // Copy to output
        if let Some(ref mut fb) = self.output {
            let width = self.width;
            let height = self.height;
            for rect in &self.dirty_regions {
                // Inline copy_to_output logic to avoid borrowing self
                for y in rect.y.max(0) as u32..(rect.bottom().min(height as i32) as u32) {
                    for x in rect.x.max(0) as u32..(rect.right().min(width as i32) as u32) {
                        let offset = ((y * width + x) * 4) as usize;
                        let color = Color {
                            b: self.compose_buffer[offset],
                            g: self.compose_buffer[offset + 1],
                            r: self.compose_buffer[offset + 2],
                            a: self.compose_buffer[offset + 3],
                        };
                        fb.put_pixel(x, y, color);
                    }
                }
            }

            if self.vsync {
                fb.vsync_wait();
            }

            if self.double_buffer {
                fb.flip();
            }
        }

        self.dirty_regions.clear();
    }

    /// Compose a specific region
    fn compose_region(&mut self, region: &Rect) {
        // Clear region with background
        for y in region.y.max(0) as u32..(region.bottom().min(self.height as i32) as u32) {
            for x in region.x.max(0) as u32..(region.right().min(self.width as i32) as u32) {
                self.set_compose_pixel(x, y, self.background);
            }
        }

        // Composite each visible layer using indexed iteration to avoid borrow conflicts
        for i in 0..self.layers.len() {
            if !self.layers[i].visible {
                continue;
            }

            let layer_bounds = self.layers[i].bounds();
            if !layer_bounds.intersects(region) {
                continue;
            }

            // Get intersection
            if let Some(intersection) = layer_bounds.intersection(region) {
                // Inline composite_layer logic to allow borrowing self.layers and self.compose_buffer
                let layer_x = self.layers[i].position.x;
                let layer_y = self.layers[i].position.y;
                let layer_opacity = self.layers[i].opacity;
                let layer_mode = self.layers[i].mode;

                for y in intersection.y.max(0) as u32..(intersection.bottom().min(self.height as i32) as u32) {
                    for x in intersection.x.max(0) as u32..(intersection.right().min(self.width as i32) as u32) {
                        let src_x = (x as i32 - layer_x) as u32;
                        let src_y = (y as i32 - layer_y) as u32;
                        let src = self.layers[i].get_pixel(src_x, src_y);

                        let src = Color {
                            r: src.r,
                            g: src.g,
                            b: src.b,
                            a: ((src.a as u32 * layer_opacity as u32) / 255) as u8,
                        };

                        if src.a == 0 {
                            continue;
                        }

                        let dst = self.get_compose_pixel(x, y);
                        let result = self.blend_pixels(src, dst, layer_mode);
                        self.set_compose_pixel(x, y, result);
                    }
                }
            }
        }
    }

    /// Composite a single layer
    fn composite_layer(&mut self, layer: &CompositorLayer, region: &Rect) {
        let layer_x = layer.position.x;
        let layer_y = layer.position.y;

        for y in region.y.max(0) as u32..(region.bottom().min(self.height as i32) as u32) {
            for x in region.x.max(0) as u32..(region.right().min(self.width as i32) as u32) {
                // Get source pixel from layer
                let src_x = (x as i32 - layer_x) as u32;
                let src_y = (y as i32 - layer_y) as u32;
                let src = layer.get_pixel(src_x, src_y);

                // Apply layer opacity
                let src = Color {
                    r: src.r,
                    g: src.g,
                    b: src.b,
                    a: ((src.a as u32 * layer.opacity as u32) / 255) as u8,
                };

                // Skip fully transparent pixels
                if src.a == 0 {
                    continue;
                }

                // Get destination pixel
                let dst = self.get_compose_pixel(x, y);

                // Blend based on composition mode
                let result = self.blend_pixels(src, dst, layer.mode);
                self.set_compose_pixel(x, y, result);
            }
        }
    }

    /// Blend two pixels
    fn blend_pixels(&self, src: Color, dst: Color, mode: CompositionMode) -> Color {
        match mode {
            CompositionMode::SourceOver => self.blend_source_over(src, dst),
            CompositionMode::Multiply => self.blend_multiply(src, dst),
            CompositionMode::Screen => self.blend_screen(src, dst),
            CompositionMode::Additive => self.blend_additive(src, dst),
            CompositionMode::SourceAtop => self.blend_source_atop(src, dst),
            CompositionMode::DestinationOver => self.blend_source_over(dst, src),
            CompositionMode::Xor => self.blend_xor(src, dst),
        }
    }

    /// Source-over blend (standard alpha blending)
    fn blend_source_over(&self, src: Color, dst: Color) -> Color {
        if src.a == 255 {
            return src;
        }
        if src.a == 0 {
            return dst;
        }

        let src_a = src.a as u32;
        let dst_a = dst.a as u32;
        let inv_src_a = 255 - src_a;

        let out_a = src_a + (dst_a * inv_src_a) / 255;
        if out_a == 0 {
            return Color::TRANSPARENT;
        }

        let r = (src.r as u32 * src_a + dst.r as u32 * dst_a * inv_src_a / 255) / out_a;
        let g = (src.g as u32 * src_a + dst.g as u32 * dst_a * inv_src_a / 255) / out_a;
        let b = (src.b as u32 * src_a + dst.b as u32 * dst_a * inv_src_a / 255) / out_a;

        Color {
            r: r.min(255) as u8,
            g: g.min(255) as u8,
            b: b.min(255) as u8,
            a: out_a.min(255) as u8,
        }
    }

    /// Multiply blend
    fn blend_multiply(&self, src: Color, dst: Color) -> Color {
        Color {
            r: ((src.r as u32 * dst.r as u32) / 255) as u8,
            g: ((src.g as u32 * dst.g as u32) / 255) as u8,
            b: ((src.b as u32 * dst.b as u32) / 255) as u8,
            a: ((src.a as u32 * dst.a as u32) / 255) as u8,
        }
    }

    /// Screen blend
    fn blend_screen(&self, src: Color, dst: Color) -> Color {
        Color {
            r: (255 - ((255 - src.r as u32) * (255 - dst.r as u32) / 255)) as u8,
            g: (255 - ((255 - src.g as u32) * (255 - dst.g as u32) / 255)) as u8,
            b: (255 - ((255 - src.b as u32) * (255 - dst.b as u32) / 255)) as u8,
            a: src.a.max(dst.a),
        }
    }

    /// Additive blend
    fn blend_additive(&self, src: Color, dst: Color) -> Color {
        Color {
            r: (src.r as u32 + dst.r as u32).min(255) as u8,
            g: (src.g as u32 + dst.g as u32).min(255) as u8,
            b: (src.b as u32 + dst.b as u32).min(255) as u8,
            a: (src.a as u32 + dst.a as u32).min(255) as u8,
        }
    }

    /// Source-atop blend
    fn blend_source_atop(&self, src: Color, dst: Color) -> Color {
        let dst_a = dst.a as u32;
        Color {
            r: ((src.r as u32 * dst_a + dst.r as u32 * (255 - src.a as u32)) / 255) as u8,
            g: ((src.g as u32 * dst_a + dst.g as u32 * (255 - src.a as u32)) / 255) as u8,
            b: ((src.b as u32 * dst_a + dst.b as u32 * (255 - src.a as u32)) / 255) as u8,
            a: dst.a,
        }
    }

    /// XOR blend
    fn blend_xor(&self, src: Color, dst: Color) -> Color {
        let src_a = src.a as u32;
        let dst_a = dst.a as u32;
        let out_a = src_a + dst_a - 2 * src_a * dst_a / 255;

        if out_a == 0 {
            return Color::TRANSPARENT;
        }

        let r = (src.r as u32 * src_a * (255 - dst_a) + dst.r as u32 * dst_a * (255 - src_a)) / (255 * out_a);
        let g = (src.g as u32 * src_a * (255 - dst_a) + dst.g as u32 * dst_a * (255 - src_a)) / (255 * out_a);
        let b = (src.b as u32 * src_a * (255 - dst_a) + dst.b as u32 * dst_a * (255 - src_a)) / (255 * out_a);

        Color {
            r: r.min(255) as u8,
            g: g.min(255) as u8,
            b: b.min(255) as u8,
            a: out_a.min(255) as u8,
        }
    }

    /// Get pixel from composition buffer
    fn get_compose_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.width || y >= self.height {
            return Color::TRANSPARENT;
        }

        let offset = ((y * self.width + x) * 4) as usize;
        Color {
            b: self.compose_buffer[offset],
            g: self.compose_buffer[offset + 1],
            r: self.compose_buffer[offset + 2],
            a: self.compose_buffer[offset + 3],
        }
    }

    /// Set pixel in composition buffer
    fn set_compose_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = ((y * self.width + x) * 4) as usize;
        self.compose_buffer[offset] = color.b;
        self.compose_buffer[offset + 1] = color.g;
        self.compose_buffer[offset + 2] = color.r;
        self.compose_buffer[offset + 3] = color.a;
    }

    /// Copy composition buffer to output framebuffer
    fn copy_to_output(&self, fb: &mut Framebuffer, region: &Rect) {
        for y in region.y.max(0) as u32..(region.bottom().min(self.height as i32) as u32) {
            for x in region.x.max(0) as u32..(region.right().min(self.width as i32) as u32) {
                let color = self.get_compose_pixel(x, y);
                fb.put_pixel(x, y, color);
            }
        }
    }

    /// Apply box blur to a region
    pub fn apply_blur(&mut self, region: &Rect, radius: u32) {
        if radius == 0 {
            return;
        }

        let r = radius as i32;
        let kernel_size = (2 * radius + 1) as u32;
        let divisor = kernel_size * kernel_size;

        // Create temp buffer
        let mut temp = self.compose_buffer.clone();

        for y in region.y.max(0) as u32..(region.bottom().min(self.height as i32) as u32) {
            for x in region.x.max(0) as u32..(region.right().min(self.width as i32) as u32) {
                let mut sum_r = 0u32;
                let mut sum_g = 0u32;
                let mut sum_b = 0u32;
                let mut sum_a = 0u32;

                for ky in -r..=r {
                    for kx in -r..=r {
                        let sx = (x as i32 + kx).clamp(0, self.width as i32 - 1) as u32;
                        let sy = (y as i32 + ky).clamp(0, self.height as i32 - 1) as u32;
                        let pixel = self.get_compose_pixel(sx, sy);
                        sum_b += pixel.b as u32;
                        sum_g += pixel.g as u32;
                        sum_r += pixel.r as u32;
                        sum_a += pixel.a as u32;
                    }
                }

                let offset = ((y * self.width + x) * 4) as usize;
                temp[offset] = (sum_b / divisor) as u8;
                temp[offset + 1] = (sum_g / divisor) as u8;
                temp[offset + 2] = (sum_r / divisor) as u8;
                temp[offset + 3] = (sum_a / divisor) as u8;
            }
        }

        self.compose_buffer = temp;
    }

    /// Apply shadow effect
    pub fn apply_shadow(&mut self, layer: &CompositorLayer, offset_x: i32, offset_y: i32, blur: u32, color: Color) {
        // Create shadow layer
        let bounds = layer.bounds();
        let shadow_rect = bounds.offset(offset_x, offset_y);

        // Draw shadow
        for y in shadow_rect.y.max(0) as u32..(shadow_rect.bottom().min(self.height as i32) as u32) {
            for x in shadow_rect.x.max(0) as u32..(shadow_rect.right().min(self.width as i32) as u32) {
                let src_x = (x as i32 - shadow_rect.x) as u32;
                let src_y = (y as i32 - shadow_rect.y) as u32;
                let alpha = layer.get_pixel(src_x, src_y).a;

                if alpha > 0 {
                    let shadow_color = Color {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: ((alpha as u32 * color.a as u32) / 255) as u8,
                    };
                    let dst = self.get_compose_pixel(x, y);
                    let result = self.blend_source_over(shadow_color, dst);
                    self.set_compose_pixel(x, y, result);
                }
            }
        }

        // Apply blur to shadow area
        if blur > 0 {
            self.apply_blur(&shadow_rect, blur);
        }
    }
}

/// Animation timing function
#[derive(Clone, Copy, Debug)]
pub enum EasingFunction {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bounce,
    Elastic,
}

impl EasingFunction {
    /// Calculate eased value (t is 0.0 to 1.0)
    pub fn ease(&self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => t * (2.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            Self::Bounce => {
                let t = 1.0 - t;
                let n1 = 7.5625;
                let d1 = 2.75;
                let result = if t < 1.0 / d1 {
                    n1 * t * t
                } else if t < 2.0 / d1 {
                    let t = t - 1.5 / d1;
                    n1 * t * t + 0.75
                } else if t < 2.5 / d1 {
                    let t = t - 2.25 / d1;
                    n1 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / d1;
                    n1 * t * t + 0.984375
                };
                1.0 - result
            }
            Self::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    let t = t - 1.0;
                    -(2.0_f32.powf(10.0 * t) * ((t - s) * (2.0 * core::f32::consts::PI) / p).sin())
                }
            }
        }
    }
}

/// Animation state
pub struct Animation {
    /// Start value
    pub start: f32,
    /// End value
    pub end: f32,
    /// Duration in milliseconds
    pub duration: u32,
    /// Elapsed time
    pub elapsed: u32,
    /// Easing function
    pub easing: EasingFunction,
    /// Is animation complete
    pub complete: bool,
}

impl Animation {
    /// Create a new animation
    pub fn new(start: f32, end: f32, duration: u32, easing: EasingFunction) -> Self {
        Self {
            start,
            end,
            duration,
            elapsed: 0,
            easing,
            complete: false,
        }
    }

    /// Update animation by delta time (ms)
    pub fn update(&mut self, delta_ms: u32) -> f32 {
        self.elapsed += delta_ms;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.complete = true;
        }

        let t = self.elapsed as f32 / self.duration as f32;
        let eased_t = self.easing.ease(t);
        self.start + (self.end - self.start) * eased_t
    }

    /// Get current value
    pub fn value(&self) -> f32 {
        let t = self.elapsed as f32 / self.duration as f32;
        let eased_t = self.easing.ease(t);
        self.start + (self.end - self.start) * eased_t
    }

    /// Reset animation
    pub fn reset(&mut self) {
        self.elapsed = 0;
        self.complete = false;
    }

    /// Reverse animation
    pub fn reverse(&mut self) {
        core::mem::swap(&mut self.start, &mut self.end);
        self.elapsed = self.duration.saturating_sub(self.elapsed);
        self.complete = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blend_source_over() {
        let compositor = Compositor::new(100, 100);
        let src = Color::rgba(255, 0, 0, 128);
        let dst = Color::rgba(0, 0, 255, 255);
        let result = compositor.blend_source_over(src, dst);

        // Red should be mixed with blue
        assert!(result.r > 0);
        assert!(result.b > 0);
    }

    #[test]
    fn test_animation() {
        let mut anim = Animation::new(0.0, 100.0, 1000, EasingFunction::Linear);
        assert_eq!(anim.update(500), 50.0);
        assert!(!anim.complete);
        assert_eq!(anim.update(500), 100.0);
        assert!(anim.complete);
    }
}
