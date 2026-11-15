//! BuildOS GUI Widget System
//!
//! Base widget types and common UI controls.

#![allow(dead_code)]

use super::{Color, Point, Rect, Size};
use super::event::Event;
use crate::drivers::graphics::Framebuffer;
use crate::drivers::graphics::font::{FontRenderer, VgaFont};
use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;

/// Widget ID
pub type WidgetId = u32;

/// Widget state flags
#[derive(Clone, Copy, Debug, Default)]
pub struct WidgetState {
    pub enabled: bool,
    pub visible: bool,
    pub focused: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub checked: bool,
}

impl WidgetState {
    pub fn normal() -> Self {
        Self {
            enabled: true,
            visible: true,
            focused: false,
            hovered: false,
            pressed: false,
            checked: false,
        }
    }
}

/// Widget trait for all UI elements
pub trait Widget {
    /// Get widget ID
    fn id(&self) -> WidgetId;

    /// Get widget bounds
    fn bounds(&self) -> Rect;

    /// Set widget bounds
    fn set_bounds(&mut self, bounds: Rect);

    /// Get widget state
    fn state(&self) -> WidgetState;

    /// Set widget state
    fn set_state(&mut self, state: WidgetState);

    /// Is widget enabled
    fn is_enabled(&self) -> bool {
        self.state().enabled
    }

    /// Is widget visible
    fn is_visible(&self) -> bool {
        self.state().visible
    }

    /// Handle event
    fn handle_event(&mut self, _event: &Event) -> bool {
        false
    }

    /// Paint widget
    fn paint(&self, fb: &mut Framebuffer);

    /// Preferred size
    fn preferred_size(&self) -> Size {
        Size::new(100, 30)
    }
}

/// Base widget implementation
pub struct BaseWidget {
    pub id: WidgetId,
    pub bounds: Rect,
    pub state: WidgetState,
    pub background: Color,
    pub foreground: Color,
    pub border_color: Color,
    pub border_width: u32,
    pub padding: u32,
}

impl BaseWidget {
    pub fn new(id: WidgetId) -> Self {
        Self {
            id,
            bounds: Rect::zero(),
            state: WidgetState::normal(),
            background: Color::rgb(240, 240, 240),
            foreground: Color::BLACK,
            border_color: Color::rgb(128, 128, 128),
            border_width: 1,
            padding: 4,
        }
    }

    pub fn content_rect(&self) -> Rect {
        let inset = (self.border_width + self.padding) as i32;
        self.bounds.inset(inset)
    }
}

/// Label widget for displaying text
pub struct Label {
    base: BaseWidget,
    text: String,
    align: TextAlign,
}

#[derive(Clone, Copy, Debug)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Label {
    pub fn new(id: WidgetId, text: &str) -> Self {
        Self {
            base: BaseWidget::new(id),
            text: String::from(text),
            align: TextAlign::Left,
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = String::from(text);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_align(&mut self, align: TextAlign) {
        self.align = align;
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();

        // Draw background
        fb.fill_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            self.base.background,
        );

        // Draw text
        let content = self.base.content_rect();
        let mut renderer = FontRenderer::new();
        renderer.set_fg_color(self.base.foreground);

        let text_width = self.text.len() as u32 * VgaFont::WIDTH as u32;
        let x = match self.align {
            TextAlign::Left => content.x,
            TextAlign::Center => content.x + ((content.width as i32 - text_width as i32) / 2),
            TextAlign::Right => content.x + content.width as i32 - text_width as i32,
        };

        let y = content.y + (content.height as i32 - VgaFont::HEIGHT as i32) / 2;
        renderer.draw_text_at(fb, x.max(0) as u32, y.max(0) as u32, &self.text);
    }

    fn preferred_size(&self) -> Size {
        Size::new(
            self.text.len() as u32 * VgaFont::WIDTH as u32 + self.base.padding * 2,
            VgaFont::HEIGHT as u32 + self.base.padding * 2,
        )
    }
}

/// Button widget
pub struct Button {
    base: BaseWidget,
    text: String,
    on_click: Option<Box<dyn Fn() + Send>>,
}

impl Button {
    pub fn new(id: WidgetId, text: &str) -> Self {
        Self {
            base: BaseWidget::new(id),
            text: String::from(text),
            on_click: None,
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = String::from(text);
    }

    pub fn set_on_click<F: Fn() + Send + 'static>(&mut self, callback: F) {
        self.on_click = Some(Box::new(callback));
    }

    fn click(&self) {
        if let Some(ref callback) = self.on_click {
            callback();
        }
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_enabled() {
            return false;
        }

        match event {
            Event::MouseDown(mouse) => {
                if self.bounds().contains(mouse.position) {
                    self.base.state.pressed = true;
                    return true;
                }
            }
            Event::MouseUp(mouse) => {
                if self.base.state.pressed {
                    self.base.state.pressed = false;
                    if self.bounds().contains(mouse.position) {
                        self.click();
                    }
                    return true;
                }
            }
            Event::MouseMove(mouse) => {
                let was_hovered = self.base.state.hovered;
                self.base.state.hovered = self.bounds().contains(mouse.position);
                if was_hovered != self.base.state.hovered {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();
        let state = self.state();

        // Determine colors based on state
        let (bg, border) = if !state.enabled {
            (Color::rgb(200, 200, 200), Color::rgb(150, 150, 150))
        } else if state.pressed {
            (Color::rgb(180, 180, 220), Color::rgb(100, 100, 150))
        } else if state.hovered {
            (Color::rgb(220, 220, 250), Color::rgb(100, 100, 150))
        } else {
            (Color::rgb(230, 230, 230), Color::rgb(128, 128, 128))
        };

        // Draw background
        fb.fill_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            bg,
        );

        // Draw border
        fb.draw_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            border,
        );

        // Draw text
        let content = self.base.content_rect();
        let mut renderer = FontRenderer::new();
        renderer.set_fg_color(if state.enabled { Color::BLACK } else { Color::rgb(128, 128, 128) });

        let text_width = self.text.len() as u32 * VgaFont::WIDTH as u32;
        let x = content.x + ((content.width as i32 - text_width as i32) / 2);
        let y = content.y + (content.height as i32 - VgaFont::HEIGHT as i32) / 2;
        renderer.draw_text_at(fb, x.max(0) as u32, y.max(0) as u32, &self.text);
    }

    fn preferred_size(&self) -> Size {
        Size::new(
            self.text.len() as u32 * VgaFont::WIDTH as u32 + self.base.padding * 4,
            VgaFont::HEIGHT as u32 + self.base.padding * 2,
        )
    }
}

/// Checkbox widget
pub struct Checkbox {
    base: BaseWidget,
    text: String,
    checked: bool,
    on_change: Option<Box<dyn Fn(bool) + Send>>,
}

impl Checkbox {
    pub fn new(id: WidgetId, text: &str) -> Self {
        Self {
            base: BaseWidget::new(id),
            text: String::from(text),
            checked: false,
            on_change: None,
        }
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn set_on_change<F: Fn(bool) + Send + 'static>(&mut self, callback: F) {
        self.on_change = Some(Box::new(callback));
    }

    fn toggle(&mut self) {
        self.checked = !self.checked;
        if let Some(ref callback) = self.on_change {
            callback(self.checked);
        }
    }
}

impl Widget for Checkbox {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_enabled() {
            return false;
        }

        if let Event::MouseUp(mouse) = event {
            if self.bounds().contains(mouse.position) {
                self.toggle();
                return true;
            }
        }
        false
    }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();
        let box_size = 16u32;
        let box_x = bounds.x.max(0) as u32;
        let box_y = bounds.y.max(0) as u32 + (bounds.height - box_size) / 2;

        // Draw checkbox box
        fb.fill_rect(box_x, box_y, box_size, box_size, Color::WHITE);
        fb.draw_rect(box_x, box_y, box_size, box_size, Color::rgb(128, 128, 128));

        // Draw check mark if checked
        if self.checked {
            // Draw X or checkmark
            let color = Color::rgb(0, 128, 0);
            for i in 2..14u32 {
                fb.put_pixel(box_x + i, box_y + i, color);
                fb.put_pixel(box_x + i + 1, box_y + i, color);
                fb.put_pixel(box_x + 15 - i, box_y + i, color);
                fb.put_pixel(box_x + 14 - i, box_y + i, color);
            }
        }

        // Draw text
        let text_x = box_x + box_size + 8;
        let text_y = bounds.y.max(0) as u32 + (bounds.height - VgaFont::HEIGHT as u32) / 2;
        let mut renderer = FontRenderer::new();
        renderer.set_fg_color(self.base.foreground);
        renderer.draw_text_at(fb, text_x, text_y, &self.text);
    }

    fn preferred_size(&self) -> Size {
        Size::new(
            16 + 8 + self.text.len() as u32 * VgaFont::WIDTH as u32 + self.base.padding * 2,
            20,
        )
    }
}

/// Text input widget
pub struct TextInput {
    base: BaseWidget,
    text: String,
    placeholder: String,
    cursor_pos: usize,
    selection_start: Option<usize>,
    max_length: usize,
    password_mode: bool,
    on_change: Option<Box<dyn Fn(&str) + Send>>,
    on_submit: Option<Box<dyn Fn(&str) + Send>>,
}

impl TextInput {
    pub fn new(id: WidgetId) -> Self {
        Self {
            base: BaseWidget::new(id),
            text: String::new(),
            placeholder: String::new(),
            cursor_pos: 0,
            selection_start: None,
            max_length: 256,
            password_mode: false,
            on_change: None,
            on_submit: None,
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = String::from(text);
        self.cursor_pos = self.text.len();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_placeholder(&mut self, placeholder: &str) {
        self.placeholder = String::from(placeholder);
    }

    pub fn set_password_mode(&mut self, enabled: bool) {
        self.password_mode = enabled;
    }

    pub fn set_on_change<F: Fn(&str) + Send + 'static>(&mut self, callback: F) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn set_on_submit<F: Fn(&str) + Send + 'static>(&mut self, callback: F) {
        self.on_submit = Some(Box::new(callback));
    }

    fn insert_char(&mut self, c: char) {
        if self.text.len() < self.max_length {
            self.text.insert(self.cursor_pos, c);
            self.cursor_pos += 1;
            self.notify_change();
        }
    }

    fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.text.remove(self.cursor_pos);
            self.notify_change();
        }
    }

    fn notify_change(&self) {
        if let Some(ref callback) = self.on_change {
            callback(&self.text);
        }
    }

    fn notify_submit(&self) {
        if let Some(ref callback) = self.on_submit {
            callback(&self.text);
        }
    }

    fn display_text(&self) -> String {
        if self.password_mode {
            "*".repeat(self.text.len())
        } else {
            self.text.clone()
        }
    }
}

impl Widget for TextInput {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_enabled() {
            return false;
        }

        match event {
            Event::MouseDown(mouse) => {
                let was_focused = self.base.state.focused;
                self.base.state.focused = self.bounds().contains(mouse.position);
                return was_focused != self.base.state.focused;
            }
            Event::KeyDown(key) if self.base.state.focused => {
                use super::event::KeyCode;
                match key.key {
                    KeyCode::Backspace => {
                        self.delete_char();
                        return true;
                    }
                    KeyCode::Delete => {
                        if self.cursor_pos < self.text.len() {
                            self.text.remove(self.cursor_pos);
                            self.notify_change();
                        }
                        return true;
                    }
                    KeyCode::Left => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                        }
                        return true;
                    }
                    KeyCode::Right => {
                        if self.cursor_pos < self.text.len() {
                            self.cursor_pos += 1;
                        }
                        return true;
                    }
                    KeyCode::Home => {
                        self.cursor_pos = 0;
                        return true;
                    }
                    KeyCode::End => {
                        self.cursor_pos = self.text.len();
                        return true;
                    }
                    KeyCode::Enter => {
                        self.notify_submit();
                        return true;
                    }
                    _ => {}
                }
            }
            Event::TextInput(c) if self.base.state.focused => {
                if !c.is_control() {
                    self.insert_char(*c);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();
        let state = self.state();

        // Draw background
        let bg = if state.focused {
            Color::WHITE
        } else {
            Color::rgb(250, 250, 250)
        };
        fb.fill_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            bg,
        );

        // Draw border
        let border = if state.focused {
            Color::rgb(0, 120, 212)
        } else {
            Color::rgb(128, 128, 128)
        };
        fb.draw_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            border,
        );

        // Draw text or placeholder
        let content = self.base.content_rect();
        let mut renderer = FontRenderer::new();

        if self.text.is_empty() && !self.placeholder.is_empty() {
            renderer.set_fg_color(Color::rgb(160, 160, 160));
            renderer.draw_text_at(
                fb,
                content.x.max(0) as u32,
                content.y.max(0) as u32,
                &self.placeholder,
            );
        } else {
            renderer.set_fg_color(self.base.foreground);
            let display = self.display_text();
            renderer.draw_text_at(
                fb,
                content.x.max(0) as u32,
                content.y.max(0) as u32,
                &display,
            );

            // Draw cursor if focused
            if state.focused {
                let cursor_x = content.x.max(0) as u32 + self.cursor_pos as u32 * VgaFont::WIDTH as u32;
                let cursor_y = content.y.max(0) as u32;
                fb.draw_vline(cursor_x, cursor_y, VgaFont::HEIGHT as u32, Color::BLACK);
            }
        }
    }

    fn preferred_size(&self) -> Size {
        Size::new(200, VgaFont::HEIGHT as u32 + self.base.padding * 2 + 4)
    }
}

/// Progress bar widget
pub struct ProgressBar {
    base: BaseWidget,
    value: f32,
    min: f32,
    max: f32,
    show_text: bool,
    bar_color: Color,
}

impl ProgressBar {
    pub fn new(id: WidgetId) -> Self {
        Self {
            base: BaseWidget::new(id),
            value: 0.0,
            min: 0.0,
            max: 100.0,
            show_text: true,
            bar_color: Color::rgb(0, 120, 212),
        }
    }

    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(self.min, self.max);
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn set_range(&mut self, min: f32, max: f32) {
        self.min = min;
        self.max = max;
        self.value = self.value.clamp(min, max);
    }

    pub fn percent(&self) -> f32 {
        (self.value - self.min) / (self.max - self.min)
    }
}

impl Widget for ProgressBar {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();

        // Draw background
        fb.fill_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            Color::rgb(220, 220, 220),
        );

        // Draw border
        fb.draw_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            Color::rgb(128, 128, 128),
        );

        // Draw progress bar
        let progress_width = ((bounds.width - 2) as f32 * self.percent()) as u32;
        if progress_width > 0 {
            fb.fill_rect(
                bounds.x.max(0) as u32 + 1,
                bounds.y.max(0) as u32 + 1,
                progress_width,
                bounds.height - 2,
                self.bar_color,
            );
        }

        // Draw percentage text
        if self.show_text {
            let text = alloc::format!("{}%", (self.percent() * 100.0) as i32);
            let text_width = text.len() as u32 * VgaFont::WIDTH as u32;
            let text_x = bounds.x.max(0) as u32 + (bounds.width - text_width) / 2;
            let text_y = bounds.y.max(0) as u32 + (bounds.height - VgaFont::HEIGHT as u32) / 2;

            let mut renderer = FontRenderer::new();
            renderer.set_fg_color(if self.percent() > 0.5 { Color::WHITE } else { Color::BLACK });
            renderer.draw_text_at(fb, text_x, text_y, &text);
        }
    }

    fn preferred_size(&self) -> Size {
        Size::new(200, 24)
    }
}

/// Slider widget
pub struct Slider {
    base: BaseWidget,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    orientation: Orientation,
    on_change: Option<Box<dyn Fn(f32) + Send>>,
    dragging: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Slider {
    pub fn new(id: WidgetId) -> Self {
        Self {
            base: BaseWidget::new(id),
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            orientation: Orientation::Horizontal,
            on_change: None,
            dragging: false,
        }
    }

    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(self.min, self.max);
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn set_range(&mut self, min: f32, max: f32) {
        self.min = min;
        self.max = max;
        self.value = self.value.clamp(min, max);
    }

    pub fn set_on_change<F: Fn(f32) + Send + 'static>(&mut self, callback: F) {
        self.on_change = Some(Box::new(callback));
    }

    fn percent(&self) -> f32 {
        (self.value - self.min) / (self.max - self.min)
    }

    fn value_from_position(&self, pos: Point) -> f32 {
        let bounds = self.bounds();
        let ratio = match self.orientation {
            Orientation::Horizontal => {
                ((pos.x - bounds.x) as f32 / bounds.width as f32).clamp(0.0, 1.0)
            }
            Orientation::Vertical => {
                1.0 - ((pos.y - bounds.y) as f32 / bounds.height as f32).clamp(0.0, 1.0)
            }
        };
        self.min + ratio * (self.max - self.min)
    }

    fn notify_change(&self) {
        if let Some(ref callback) = self.on_change {
            callback(self.value);
        }
    }
}

impl Widget for Slider {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_enabled() {
            return false;
        }

        match event {
            Event::MouseDown(mouse) => {
                if self.bounds().contains(mouse.position) {
                    self.dragging = true;
                    self.value = self.value_from_position(mouse.position);
                    self.notify_change();
                    return true;
                }
            }
            Event::MouseUp(_) => {
                if self.dragging {
                    self.dragging = false;
                    return true;
                }
            }
            Event::MouseMove(mouse) => {
                if self.dragging {
                    self.value = self.value_from_position(mouse.position);
                    self.notify_change();
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();

        // Draw track
        let track_thickness = 4u32;
        let (track_x, track_y, track_w, track_h) = match self.orientation {
            Orientation::Horizontal => (
                bounds.x.max(0) as u32,
                bounds.y.max(0) as u32 + (bounds.height - track_thickness) / 2,
                bounds.width,
                track_thickness,
            ),
            Orientation::Vertical => (
                bounds.x.max(0) as u32 + (bounds.width - track_thickness) / 2,
                bounds.y.max(0) as u32,
                track_thickness,
                bounds.height,
            ),
        };

        fb.fill_rect(track_x, track_y, track_w, track_h, Color::rgb(200, 200, 200));
        fb.draw_rect(track_x, track_y, track_w, track_h, Color::rgb(128, 128, 128));

        // Draw thumb
        let thumb_size = 16u32;
        let (thumb_x, thumb_y) = match self.orientation {
            Orientation::Horizontal => {
                let pos = ((bounds.width - thumb_size) as f32 * self.percent()) as u32;
                (bounds.x.max(0) as u32 + pos, bounds.y.max(0) as u32 + (bounds.height - thumb_size) / 2)
            }
            Orientation::Vertical => {
                let pos = ((bounds.height - thumb_size) as f32 * (1.0 - self.percent())) as u32;
                (bounds.x.max(0) as u32 + (bounds.width - thumb_size) / 2, bounds.y.max(0) as u32 + pos)
            }
        };

        let thumb_color = if self.dragging {
            Color::rgb(0, 100, 180)
        } else if self.base.state.hovered {
            Color::rgb(0, 120, 212)
        } else {
            Color::rgb(60, 60, 60)
        };

        fb.fill_rect(thumb_x, thumb_y, thumb_size, thumb_size, thumb_color);
        fb.draw_rect(thumb_x, thumb_y, thumb_size, thumb_size, Color::rgb(40, 40, 40));
    }

    fn preferred_size(&self) -> Size {
        match self.orientation {
            Orientation::Horizontal => Size::new(200, 24),
            Orientation::Vertical => Size::new(24, 200),
        }
    }
}

/// List box widget
pub struct ListBox {
    base: BaseWidget,
    items: Vec<String>,
    selected_index: Option<usize>,
    scroll_offset: usize,
    visible_items: usize,
    on_select: Option<Box<dyn Fn(usize, &str) + Send>>,
}

impl ListBox {
    pub fn new(id: WidgetId) -> Self {
        Self {
            base: BaseWidget::new(id),
            items: Vec::new(),
            selected_index: None,
            scroll_offset: 0,
            visible_items: 10,
            on_select: None,
        }
    }

    pub fn add_item(&mut self, item: &str) {
        self.items.push(String::from(item));
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.selected_index = None;
        self.scroll_offset = 0;
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn selected_item(&self) -> Option<&str> {
        self.selected_index.and_then(|i| self.items.get(i).map(|s| s.as_str()))
    }

    pub fn set_on_select<F: Fn(usize, &str) + Send + 'static>(&mut self, callback: F) {
        self.on_select = Some(Box::new(callback));
    }

    fn item_height(&self) -> u32 {
        VgaFont::HEIGHT as u32 + 4
    }

    fn item_at_y(&self, y: i32) -> Option<usize> {
        let bounds = self.bounds();
        if y < bounds.y || y >= bounds.bottom() {
            return None;
        }

        let item_h = self.item_height() as i32;
        let index = ((y - bounds.y) / item_h) as usize + self.scroll_offset;
        if index < self.items.len() {
            Some(index)
        } else {
            None
        }
    }
}

impl Widget for ListBox {
    fn id(&self) -> WidgetId { self.base.id }
    fn bounds(&self) -> Rect { self.base.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.base.bounds = bounds; }
    fn state(&self) -> WidgetState { self.base.state }
    fn set_state(&mut self, state: WidgetState) { self.base.state = state; }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_enabled() {
            return false;
        }

        match event {
            Event::MouseDown(mouse) => {
                if self.bounds().contains(mouse.position) {
                    if let Some(index) = self.item_at_y(mouse.position.y) {
                        self.selected_index = Some(index);
                        if let Some(ref callback) = self.on_select {
                            callback(index, &self.items[index]);
                        }
                        return true;
                    }
                }
            }
            Event::MouseScroll(mouse) => {
                if self.bounds().contains(mouse.position) {
                    if mouse.scroll_delta.y < 0 && self.scroll_offset > 0 {
                        self.scroll_offset -= 1;
                        return true;
                    } else if mouse.scroll_delta.y > 0 && self.scroll_offset + self.visible_items < self.items.len() {
                        self.scroll_offset += 1;
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    fn paint(&self, fb: &mut Framebuffer) {
        let bounds = self.bounds();

        // Draw background
        fb.fill_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            Color::WHITE,
        );

        // Draw border
        fb.draw_rect(
            bounds.x.max(0) as u32,
            bounds.y.max(0) as u32,
            bounds.width,
            bounds.height,
            Color::rgb(128, 128, 128),
        );

        // Draw items
        let item_h = self.item_height();
        let mut renderer = FontRenderer::new();

        for i in 0..self.visible_items.min(self.items.len().saturating_sub(self.scroll_offset)) {
            let item_index = i + self.scroll_offset;
            let item_y = bounds.y.max(0) as u32 + (i as u32 * item_h);

            // Draw selection highlight
            if Some(item_index) == self.selected_index {
                fb.fill_rect(
                    bounds.x.max(0) as u32 + 1,
                    item_y,
                    bounds.width - 2,
                    item_h,
                    Color::rgb(0, 120, 212),
                );
                renderer.set_fg_color(Color::WHITE);
            } else {
                renderer.set_fg_color(Color::BLACK);
            }

            // Draw item text
            if let Some(item) = self.items.get(item_index) {
                renderer.draw_text_at(fb, bounds.x.max(0) as u32 + 4, item_y + 2, item);
            }
        }
    }

    fn preferred_size(&self) -> Size {
        Size::new(200, self.visible_items as u32 * self.item_height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label() {
        let label = Label::new(1, "Hello");
        assert_eq!(label.text(), "Hello");
    }

    #[test]
    fn test_checkbox() {
        let mut checkbox = Checkbox::new(1, "Test");
        assert!(!checkbox.is_checked());
        checkbox.set_checked(true);
        assert!(checkbox.is_checked());
    }

    #[test]
    fn test_progress_bar() {
        let mut progress = ProgressBar::new(1);
        progress.set_value(50.0);
        assert_eq!(progress.percent(), 0.5);
    }
}
