//! BuildOS GUI Subsystem
//!
//! Window manager, compositor, and GUI primitives for graphical applications.

pub mod compositor;
pub mod window;
pub mod widget;
pub mod event;
pub mod theme;

use crate::drivers::graphics::{Color, Framebuffer, DisplayMode};
use crate::sync::RwLock;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum number of windows
pub const MAX_WINDOWS: usize = 256;

/// Maximum window title length
pub const MAX_TITLE_LENGTH: usize = 128;

/// Window ID type
pub type WindowId = u32;

/// Next window ID
static NEXT_WINDOW_ID: AtomicU32 = AtomicU32::new(1);

/// GUI subsystem state
static GUI_STATE: RwLock<Option<GuiSubsystem>> = RwLock::new(None);

/// Point structure
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }
}

/// Size structure
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn zero() -> Self {
        Self { width: 0, height: 0 }
    }
}

/// Rectangle structure
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn from_points(p1: Point, p2: Point) -> Self {
        let x = if p1.x < p2.x { p1.x } else { p2.x };
        let y = if p1.y < p2.y { p1.y } else { p2.y };
        let width = (p1.x - p2.x).unsigned_abs();
        let height = (p1.y - p2.y).unsigned_abs();
        Self { x, y, width, height }
    }

    pub const fn zero() -> Self {
        Self { x: 0, y: 0, width: 0, height: 0 }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x && point.x < self.right() &&
        point.y >= self.y && point.y < self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right() && self.right() > other.x &&
        self.y < other.bottom() && self.bottom() > other.y
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        if !self.intersects(other) {
            return None;
        }

        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        Some(Rect {
            x,
            y,
            width: (right - x) as u32,
            height: (bottom - y) as u32,
        })
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());

        Rect {
            x,
            y,
            width: (right - x) as u32,
            height: (bottom - y) as u32,
        }
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect {
            x: self.x + dx,
            y: self.y + dy,
            width: self.width,
            height: self.height,
        }
    }

    pub fn inset(&self, amount: i32) -> Rect {
        let amount_u = amount.unsigned_abs();
        Rect {
            x: self.x + amount,
            y: self.y + amount,
            width: self.width.saturating_sub(amount_u * 2),
            height: self.height.saturating_sub(amount_u * 2),
        }
    }
}

/// Window flags
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowFlags {
    bits: u32,
}

impl WindowFlags {
    pub const NONE: Self = Self { bits: 0 };
    pub const VISIBLE: Self = Self { bits: 1 << 0 };
    pub const FOCUSED: Self = Self { bits: 1 << 1 };
    pub const DECORATED: Self = Self { bits: 1 << 2 };
    pub const RESIZABLE: Self = Self { bits: 1 << 3 };
    pub const MOVABLE: Self = Self { bits: 1 << 4 };
    pub const MINIMIZED: Self = Self { bits: 1 << 5 };
    pub const MAXIMIZED: Self = Self { bits: 1 << 6 };
    pub const ALWAYS_ON_TOP: Self = Self { bits: 1 << 7 };
    pub const TRANSPARENT: Self = Self { bits: 1 << 8 };
    pub const FULLSCREEN: Self = Self { bits: 1 << 9 };
    pub const MODAL: Self = Self { bits: 1 << 10 };
    pub const POPUP: Self = Self { bits: 1 << 11 };
    pub const TOOLTIP: Self = Self { bits: 1 << 12 };

    pub const DEFAULT: Self = Self {
        bits: Self::VISIBLE.bits | Self::DECORATED.bits | Self::RESIZABLE.bits | Self::MOVABLE.bits,
    };

    pub fn contains(self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    pub fn insert(&mut self, other: Self) {
        self.bits |= other.bits;
    }

    pub fn remove(&mut self, other: Self) {
        self.bits &= !other.bits;
    }

    pub fn toggle(&mut self, other: Self) {
        self.bits ^= other.bits;
    }
}

/// Window state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
    Hidden,
}

/// GUI subsystem
pub struct GuiSubsystem {
    /// Display framebuffer
    framebuffer: Option<Framebuffer>,
    /// Display mode
    display_mode: Option<DisplayMode>,
    /// Window list (ordered by z-order, front to back)
    windows: Vec<Arc<RwLock<Window>>>,
    /// Window ID map
    window_map: BTreeMap<WindowId, Arc<RwLock<Window>>>,
    /// Focused window ID
    focused_window: Option<WindowId>,
    /// Mouse position
    mouse_pos: Point,
    /// Mouse button state
    mouse_buttons: u8,
    /// Is running
    running: bool,
    /// Needs redraw
    dirty: bool,
    /// Dirty regions
    dirty_regions: Vec<Rect>,
    /// Desktop color
    desktop_color: Color,
    /// Cursor visible
    cursor_visible: bool,
    /// Cursor position
    cursor_pos: Point,
}

impl GuiSubsystem {
    /// Create a new GUI subsystem
    pub fn new() -> Self {
        Self {
            framebuffer: None,
            display_mode: None,
            windows: Vec::new(),
            window_map: BTreeMap::new(),
            focused_window: None,
            mouse_pos: Point::zero(),
            mouse_buttons: 0,
            running: false,
            dirty: true,
            dirty_regions: Vec::new(),
            desktop_color: Color::rgb(32, 64, 96), // Dark blue
            cursor_visible: true,
            cursor_pos: Point::zero(),
        }
    }

    /// Initialize the GUI subsystem
    pub fn init(&mut self, fb: Framebuffer, mode: DisplayMode) -> Result<(), GuiError> {
        self.framebuffer = Some(fb);
        self.display_mode = Some(mode);
        self.running = true;
        self.dirty = true;

        // Mark entire screen as dirty
        self.dirty_regions.push(Rect::new(0, 0, mode.width, mode.height));

        Ok(())
    }

    /// Create a new window
    pub fn create_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        flags: WindowFlags,
    ) -> Result<WindowId, GuiError> {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::SeqCst);

        let window = Window::new(id, title, Rect::new(x, y, width, height), flags);
        let window = Arc::new(RwLock::new(window));

        self.windows.push(window.clone());
        self.window_map.insert(id, window);

        // Set focus to new window
        self.focused_window = Some(id);

        // Mark window area as dirty
        self.mark_dirty(Rect::new(x, y, width, height));

        Ok(id)
    }

    /// Destroy a window
    pub fn destroy_window(&mut self, id: WindowId) -> Result<(), GuiError> {
        // Find and remove from z-order list
        self.windows.retain(|w| w.read().id != id);

        // Remove from map
        if let Some(window) = self.window_map.remove(&id) {
            let rect = window.read().frame;
            self.mark_dirty(rect);
        } else {
            return Err(GuiError::InvalidWindow);
        }

        // Update focus
        if self.focused_window == Some(id) {
            self.focused_window = self.windows.first()
                .map(|w| w.read().id);
        }

        Ok(())
    }

    /// Get window by ID
    pub fn get_window(&self, id: WindowId) -> Option<Arc<RwLock<Window>>> {
        self.window_map.get(&id).cloned()
    }

    /// Bring window to front
    pub fn bring_to_front(&mut self, id: WindowId) {
        if let Some(idx) = self.windows.iter().position(|w| w.read().id == id) {
            let window = self.windows.remove(idx);
            let rect = window.read().frame;
            self.windows.insert(0, window);
            self.mark_dirty(rect);
        }
    }

    /// Set focused window
    pub fn set_focus(&mut self, id: WindowId) {
        if let Some(old_id) = self.focused_window {
            if let Some(window) = self.get_window(old_id) {
                let mut w = window.write();
                w.flags.remove(WindowFlags::FOCUSED);
                self.mark_dirty(w.frame);
            }
        }

        if let Some(window) = self.get_window(id) {
            let mut w = window.write();
            w.flags.insert(WindowFlags::FOCUSED);
            self.focused_window = Some(id);
            self.mark_dirty(w.frame);
        }
    }

    /// Mark region as dirty
    pub fn mark_dirty(&mut self, rect: Rect) {
        self.dirty = true;

        // Merge with existing dirty regions or add new one
        let mut merged = false;
        for existing in &mut self.dirty_regions {
            if existing.intersects(&rect) {
                *existing = existing.union(&rect);
                merged = true;
                break;
            }
        }

        if !merged {
            self.dirty_regions.push(rect);
        }
    }

    /// Handle mouse move event
    pub fn handle_mouse_move(&mut self, x: i32, y: i32) {
        self.cursor_pos = Point::new(x, y);
        self.mouse_pos = Point::new(x, y);

        // Find window under cursor
        for window in &self.windows {
            let w = window.read();
            if w.flags.contains(WindowFlags::VISIBLE) && w.frame.contains(self.mouse_pos) {
                // Send hover event to window
                break;
            }
        }
    }

    /// Handle mouse button event
    pub fn handle_mouse_button(&mut self, button: u8, pressed: bool) {
        if pressed {
            self.mouse_buttons |= button;

            // Find and focus window under cursor
            for window in &self.windows {
                let w = window.read();
                if w.flags.contains(WindowFlags::VISIBLE) && w.frame.contains(self.mouse_pos) {
                    let id = w.id;
                    drop(w);
                    self.bring_to_front(id);
                    self.set_focus(id);
                    break;
                }
            }
        } else {
            self.mouse_buttons &= !button;
        }
    }

    /// Handle keyboard event
    pub fn handle_key(&mut self, key: u8, pressed: bool) {
        if let Some(id) = self.focused_window {
            if let Some(window) = self.get_window(id) {
                let _w = window.write();
                // Dispatch key event to window
                let _ = (key, pressed); // Use the values
            }
        }
    }

    /// Render all windows
    pub fn render(&mut self) {
        if !self.dirty {
            return;
        }

        let Some(_mode) = self.display_mode else { return };

        // Take framebuffer temporarily to avoid borrow conflicts
        let Some(mut fb) = self.framebuffer.take() else { return };

        // Clear dirty regions with desktop color
        for rect in &self.dirty_regions {
            fb.fill_rect(
                rect.x.max(0) as u32,
                rect.y.max(0) as u32,
                rect.width,
                rect.height,
                self.desktop_color,
            );
        }

        // Render windows back to front
        for window in self.windows.iter().rev() {
            let w = window.read();
            if !w.flags.contains(WindowFlags::VISIBLE) {
                continue;
            }

            // Check if window intersects any dirty region
            let mut needs_redraw = false;
            for rect in &self.dirty_regions {
                if w.frame.intersects(rect) {
                    needs_redraw = true;
                    break;
                }
            }

            if needs_redraw {
                self.render_window(&w, &mut fb);
            }
        }

        // Render cursor
        if self.cursor_visible {
            self.render_cursor(&mut fb);
        }

        // Flip buffers if double buffered
        fb.flip();

        // Put framebuffer back
        self.framebuffer = Some(fb);

        self.dirty = false;
        self.dirty_regions.clear();
    }

    /// Render a single window
    fn render_window(&self, window: &Window, fb: &mut Framebuffer) {
        let frame = window.frame;

        // Draw window shadow
        if window.flags.contains(WindowFlags::DECORATED) {
            fb.fill_rect(
                (frame.x + 4).max(0) as u32,
                (frame.y + 4).max(0) as u32,
                frame.width,
                frame.height,
                Color::rgba(0, 0, 0, 64),
            );
        }

        // Draw window background
        fb.fill_rect(
            frame.x.max(0) as u32,
            frame.y.max(0) as u32,
            frame.width,
            frame.height,
            window.background_color,
        );

        // Draw window decoration
        if window.flags.contains(WindowFlags::DECORATED) {
            self.render_window_decoration(window, fb);
        }

        // Draw window content
        if let Some(ref buffer) = window.content_buffer {
            let content_rect = window.content_rect();
            // Blit content buffer to framebuffer
            fb.blit_raw(
                content_rect.x.max(0) as u32,
                content_rect.y.max(0) as u32,
                buffer,
                content_rect.width,
                content_rect.height,
            );
        }
    }

    /// Render window decoration (title bar, borders, buttons)
    fn render_window_decoration(&self, window: &Window, fb: &mut Framebuffer) {
        let frame = window.frame;
        let is_focused = self.focused_window == Some(window.id);

        // Title bar colors
        let title_bar_color = if is_focused {
            Color::rgb(60, 120, 180) // Active window
        } else {
            Color::rgb(128, 128, 128) // Inactive window
        };

        // Draw title bar
        let title_height = Window::TITLE_BAR_HEIGHT;
        fb.fill_rect(
            frame.x.max(0) as u32,
            frame.y.max(0) as u32,
            frame.width,
            title_height,
            title_bar_color,
        );

        // Draw title text
        // (Would use font renderer here)

        // Draw window border
        let border_color = if is_focused {
            Color::rgb(80, 140, 200)
        } else {
            Color::rgb(96, 96, 96)
        };

        // Top border
        fb.draw_hline(frame.x.max(0) as u32, frame.y.max(0) as u32, frame.width, border_color);
        // Bottom border
        fb.draw_hline(frame.x.max(0) as u32, (frame.bottom() - 1).max(0) as u32, frame.width, border_color);
        // Left border
        fb.draw_vline(frame.x.max(0) as u32, frame.y.max(0) as u32, frame.height, border_color);
        // Right border
        fb.draw_vline((frame.right() - 1).max(0) as u32, frame.y.max(0) as u32, frame.height, border_color);

        // Draw window buttons (close, minimize, maximize)
        let button_size = 16u32;
        let button_y = frame.y + 4;
        let button_spacing = 20i32;

        // Close button (red)
        let close_x = frame.right() - button_spacing;
        fb.fill_rect(
            close_x.max(0) as u32,
            button_y.max(0) as u32,
            button_size,
            button_size,
            Color::rgb(255, 80, 80),
        );

        // Maximize button (green)
        let max_x = frame.right() - button_spacing * 2;
        fb.fill_rect(
            max_x.max(0) as u32,
            button_y.max(0) as u32,
            button_size,
            button_size,
            Color::rgb(80, 200, 80),
        );

        // Minimize button (yellow)
        let min_x = frame.right() - button_spacing * 3;
        fb.fill_rect(
            min_x.max(0) as u32,
            button_y.max(0) as u32,
            button_size,
            button_size,
            Color::rgb(255, 200, 80),
        );
    }

    /// Render mouse cursor
    fn render_cursor(&self, fb: &mut Framebuffer) {
        let x = self.cursor_pos.x.max(0) as u32;
        let y = self.cursor_pos.y.max(0) as u32;

        // Simple arrow cursor
        let cursor_data: [u16; 16] = [
            0b1000000000000000,
            0b1100000000000000,
            0b1110000000000000,
            0b1111000000000000,
            0b1111100000000000,
            0b1111110000000000,
            0b1111111000000000,
            0b1111111100000000,
            0b1111111110000000,
            0b1111100000000000,
            0b1101100000000000,
            0b1000110000000000,
            0b0000110000000000,
            0b0000011000000000,
            0b0000011000000000,
            0b0000000000000000,
        ];

        for (row, bits) in cursor_data.iter().enumerate() {
            for col in 0..16 {
                if (bits >> (15 - col)) & 1 != 0 {
                    fb.put_pixel(x + col, y + row as u32, Color::WHITE);
                }
            }
        }
    }

    /// Get screen dimensions
    pub fn screen_size(&self) -> Size {
        self.display_mode.map(|m| Size::new(m.width, m.height))
            .unwrap_or(Size::zero())
    }
}

/// Window structure
pub struct Window {
    /// Window ID
    pub id: WindowId,
    /// Window title
    pub title: String,
    /// Window frame (position and size)
    pub frame: Rect,
    /// Previous frame (before maximize/minimize)
    pub saved_frame: Rect,
    /// Window flags
    pub flags: WindowFlags,
    /// Window state
    pub state: WindowState,
    /// Background color
    pub background_color: Color,
    /// Content buffer (pixels)
    pub content_buffer: Option<Vec<u8>>,
    /// Owner process ID
    pub owner_pid: u32,
    /// Parent window ID
    pub parent: Option<WindowId>,
    /// Child windows
    pub children: Vec<WindowId>,
}

impl Window {
    /// Title bar height
    pub const TITLE_BAR_HEIGHT: u32 = 24;
    /// Border width
    pub const BORDER_WIDTH: u32 = 1;
    /// Minimum window size
    pub const MIN_SIZE: Size = Size { width: 100, height: 50 };

    /// Create a new window
    pub fn new(id: WindowId, title: &str, frame: Rect, flags: WindowFlags) -> Self {
        Self {
            id,
            title: String::from(title),
            frame,
            saved_frame: frame,
            flags,
            state: WindowState::Normal,
            background_color: Color::rgb(240, 240, 240),
            content_buffer: None,
            owner_pid: 0,
            parent: None,
            children: Vec::new(),
        }
    }

    /// Get content area rectangle
    pub fn content_rect(&self) -> Rect {
        if self.flags.contains(WindowFlags::DECORATED) {
            Rect {
                x: self.frame.x + Self::BORDER_WIDTH as i32,
                y: self.frame.y + Self::TITLE_BAR_HEIGHT as i32,
                width: self.frame.width - Self::BORDER_WIDTH * 2,
                height: self.frame.height - Self::TITLE_BAR_HEIGHT - Self::BORDER_WIDTH,
            }
        } else {
            self.frame
        }
    }

    /// Move window
    pub fn move_to(&mut self, x: i32, y: i32) {
        self.frame.x = x;
        self.frame.y = y;
    }

    /// Resize window
    pub fn resize(&mut self, width: u32, height: u32) {
        self.frame.width = width.max(Self::MIN_SIZE.width);
        self.frame.height = height.max(Self::MIN_SIZE.height);

        // Reallocate content buffer
        let content = self.content_rect();
        let buffer_size = (content.width * content.height * 4) as usize;
        self.content_buffer = Some(alloc::vec![0u8; buffer_size]);
    }

    /// Maximize window
    pub fn maximize(&mut self, screen_size: Size) {
        if self.state == WindowState::Maximized {
            // Restore
            self.frame = self.saved_frame;
            self.state = WindowState::Normal;
            self.flags.remove(WindowFlags::MAXIMIZED);
        } else {
            // Maximize
            self.saved_frame = self.frame;
            self.frame = Rect::new(0, 0, screen_size.width, screen_size.height);
            self.state = WindowState::Maximized;
            self.flags.insert(WindowFlags::MAXIMIZED);
        }
    }

    /// Minimize window
    pub fn minimize(&mut self) {
        self.state = WindowState::Minimized;
        self.flags.insert(WindowFlags::MINIMIZED);
        self.flags.remove(WindowFlags::VISIBLE);
    }

    /// Restore from minimized
    pub fn restore(&mut self) {
        self.state = WindowState::Normal;
        self.flags.remove(WindowFlags::MINIMIZED);
        self.flags.insert(WindowFlags::VISIBLE);
    }

    /// Show window
    pub fn show(&mut self) {
        self.flags.insert(WindowFlags::VISIBLE);
    }

    /// Hide window
    pub fn hide(&mut self) {
        self.flags.remove(WindowFlags::VISIBLE);
    }

    /// Set title
    pub fn set_title(&mut self, title: &str) {
        self.title = String::from(title);
    }
}

/// GUI error types
#[derive(Debug, Clone, Copy)]
pub enum GuiError {
    NotInitialized,
    InvalidWindow,
    InvalidOperation,
    OutOfMemory,
    DisplayError,
}

/// Initialize the GUI subsystem
pub fn init(fb: Framebuffer, mode: DisplayMode) -> Result<(), GuiError> {
    let mut gui = GuiSubsystem::new();
    gui.init(fb, mode)?;

    let mut state = GUI_STATE.write();
    *state = Some(gui);

    Ok(())
}

/// Create a new window
pub fn create_window(
    title: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    flags: WindowFlags,
) -> Result<WindowId, GuiError> {
    let mut state = GUI_STATE.write();
    let gui = state.as_mut().ok_or(GuiError::NotInitialized)?;
    gui.create_window(title, x, y, width, height, flags)
}

/// Destroy a window
pub fn destroy_window(id: WindowId) -> Result<(), GuiError> {
    let mut state = GUI_STATE.write();
    let gui = state.as_mut().ok_or(GuiError::NotInitialized)?;
    gui.destroy_window(id)
}

/// Get window
pub fn get_window(id: WindowId) -> Option<Arc<RwLock<Window>>> {
    let state = GUI_STATE.read();
    state.as_ref().and_then(|gui| gui.get_window(id))
}

/// Process GUI events and render
pub fn update() {
    let mut state = GUI_STATE.write();
    if let Some(gui) = state.as_mut() {
        gui.render();
    }
}

/// Handle mouse move
pub fn handle_mouse_move(x: i32, y: i32) {
    let mut state = GUI_STATE.write();
    if let Some(gui) = state.as_mut() {
        gui.handle_mouse_move(x, y);
    }
}

/// Handle mouse button
pub fn handle_mouse_button(button: u8, pressed: bool) {
    let mut state = GUI_STATE.write();
    if let Some(gui) = state.as_mut() {
        gui.handle_mouse_button(button, pressed);
    }
}

/// Handle key press
pub fn handle_key(key: u8, pressed: bool) {
    let mut state = GUI_STATE.write();
    if let Some(gui) = state.as_mut() {
        gui.handle_key(key, pressed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10, 10, 100, 100);
        assert!(rect.contains(Point::new(50, 50)));
        assert!(!rect.contains(Point::new(5, 5)));
        assert!(rect.contains(Point::new(10, 10)));
        assert!(!rect.contains(Point::new(110, 110)));
    }

    #[test]
    fn test_rect_intersection() {
        let r1 = Rect::new(0, 0, 100, 100);
        let r2 = Rect::new(50, 50, 100, 100);
        let intersection = r1.intersection(&r2).unwrap();
        assert_eq!(intersection.x, 50);
        assert_eq!(intersection.y, 50);
        assert_eq!(intersection.width, 50);
        assert_eq!(intersection.height, 50);
    }

    #[test]
    fn test_window_content_rect() {
        let window = Window::new(1, "Test", Rect::new(0, 0, 200, 200), WindowFlags::DEFAULT);
        let content = window.content_rect();
        assert_eq!(content.y, Window::TITLE_BAR_HEIGHT as i32);
    }
}
