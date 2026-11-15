//! BuildOS Window Management
//!
//! Window creation, management, and operations.

use super::{
    Point, Rect, Size, WindowId,
};
use alloc::vec::Vec;
use alloc::string::String;

/// Window manager configuration
#[derive(Clone, Debug)]
pub struct WindowManagerConfig {
    /// Snap to edges when dragging
    pub snap_to_edges: bool,
    /// Snap distance in pixels
    pub snap_distance: u32,
    /// Animate window operations
    pub animate: bool,
    /// Animation duration in ms
    pub animation_duration: u32,
    /// Show window shadows
    pub show_shadows: bool,
    /// Allow window tiling
    pub allow_tiling: bool,
    /// Minimum window width
    pub min_window_width: u32,
    /// Minimum window height
    pub min_window_height: u32,
    /// Title bar height
    pub title_bar_height: u32,
    /// Border width
    pub border_width: u32,
}

impl Default for WindowManagerConfig {
    fn default() -> Self {
        Self {
            snap_to_edges: true,
            snap_distance: 10,
            animate: true,
            animation_duration: 200,
            show_shadows: true,
            allow_tiling: true,
            min_window_width: 200,
            min_window_height: 100,
            title_bar_height: 24,
            border_width: 1,
        }
    }
}

/// Window tiling mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileMode {
    /// Not tiled
    None,
    /// Left half
    Left,
    /// Right half
    Right,
    /// Top half
    Top,
    /// Bottom half
    Bottom,
    /// Top-left quarter
    TopLeft,
    /// Top-right quarter
    TopRight,
    /// Bottom-left quarter
    BottomLeft,
    /// Bottom-right quarter
    BottomRight,
}

/// Window drag operation
#[derive(Clone, Copy, Debug)]
pub enum DragOperation {
    None,
    Move { start: Point, window_start: Point },
    ResizeNW { start: Point, window_start: Rect },
    ResizeN { start: Point, window_start: Rect },
    ResizeNE { start: Point, window_start: Rect },
    ResizeE { start: Point, window_start: Rect },
    ResizeSE { start: Point, window_start: Rect },
    ResizeS { start: Point, window_start: Rect },
    ResizeSW { start: Point, window_start: Rect },
    ResizeW { start: Point, window_start: Rect },
}

/// Resize handle positions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    None,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl ResizeHandle {
    /// Handle size for hit testing
    const SIZE: i32 = 8;

    /// Test which handle is at a point relative to window
    pub fn at_point(point: Point, window: &Rect) -> Self {
        let x = point.x;
        let y = point.y;
        let left = window.x;
        let top = window.y;
        let right = window.right();
        let bottom = window.bottom();

        let near_left = x >= left && x < left + Self::SIZE;
        let near_right = x > right - Self::SIZE && x <= right;
        let near_top = y >= top && y < top + Self::SIZE;
        let near_bottom = y > bottom - Self::SIZE && y <= bottom;

        if near_top && near_left {
            Self::NorthWest
        } else if near_top && near_right {
            Self::NorthEast
        } else if near_bottom && near_left {
            Self::SouthWest
        } else if near_bottom && near_right {
            Self::SouthEast
        } else if near_top {
            Self::North
        } else if near_bottom {
            Self::South
        } else if near_left {
            Self::West
        } else if near_right {
            Self::East
        } else {
            Self::None
        }
    }
}

/// Window stack order
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StackOrder(u32);

impl StackOrder {
    /// Desktop layer (always bottom)
    pub const DESKTOP: Self = Self(0);
    /// Normal windows
    pub const NORMAL: Self = Self(100);
    /// Floating windows
    pub const FLOATING: Self = Self(200);
    /// Always on top windows
    pub const ALWAYS_ON_TOP: Self = Self(300);
    /// System overlays
    pub const OVERLAY: Self = Self(400);
    /// Popups and menus
    pub const POPUP: Self = Self(500);
    /// Tooltips
    pub const TOOLTIP: Self = Self(600);
    /// Drag previews
    pub const DRAG: Self = Self(700);
}

/// Window manager
pub struct WindowManager {
    /// Configuration
    config: WindowManagerConfig,
    /// Screen size
    screen_size: Size,
    /// Current drag operation
    drag_op: DragOperation,
    /// Window being dragged
    drag_window: Option<WindowId>,
    /// Hot corners enabled
    hot_corners_enabled: bool,
    /// Work area (excludes taskbar, etc.)
    work_area: Rect,
}

impl WindowManager {
    /// Create a new window manager
    pub fn new(screen_size: Size) -> Self {
        let work_area = Rect::new(0, 0, screen_size.width, screen_size.height - 48); // 48px taskbar
        Self {
            config: WindowManagerConfig::default(),
            screen_size,
            drag_op: DragOperation::None,
            drag_window: None,
            hot_corners_enabled: true,
            work_area,
        }
    }

    /// Set configuration
    pub fn set_config(&mut self, config: WindowManagerConfig) {
        self.config = config;
    }

    /// Get configuration
    pub fn config(&self) -> &WindowManagerConfig {
        &self.config
    }

    /// Set screen size
    pub fn set_screen_size(&mut self, size: Size) {
        self.screen_size = size;
        self.work_area = Rect::new(0, 0, size.width, size.height.saturating_sub(48));
    }

    /// Set work area
    pub fn set_work_area(&mut self, area: Rect) {
        self.work_area = area;
    }

    /// Get work area
    pub fn work_area(&self) -> Rect {
        self.work_area
    }

    /// Calculate tile rect for mode
    pub fn tile_rect(&self, mode: TileMode) -> Rect {
        let area = self.work_area;
        let half_w = area.width / 2;
        let half_h = area.height / 2;

        match mode {
            TileMode::None => Rect::zero(),
            TileMode::Left => Rect::new(area.x, area.y, half_w, area.height),
            TileMode::Right => Rect::new(area.x + half_w as i32, area.y, half_w, area.height),
            TileMode::Top => Rect::new(area.x, area.y, area.width, half_h),
            TileMode::Bottom => Rect::new(area.x, area.y + half_h as i32, area.width, half_h),
            TileMode::TopLeft => Rect::new(area.x, area.y, half_w, half_h),
            TileMode::TopRight => Rect::new(area.x + half_w as i32, area.y, half_w, half_h),
            TileMode::BottomLeft => Rect::new(area.x, area.y + half_h as i32, half_w, half_h),
            TileMode::BottomRight => Rect::new(area.x + half_w as i32, area.y + half_h as i32, half_w, half_h),
        }
    }

    /// Start drag operation
    pub fn start_drag(&mut self, window_id: WindowId, window_rect: Rect, mouse_pos: Point, handle: ResizeHandle) {
        self.drag_window = Some(window_id);

        self.drag_op = match handle {
            ResizeHandle::None => DragOperation::Move {
                start: mouse_pos,
                window_start: Point::new(window_rect.x, window_rect.y),
            },
            ResizeHandle::North => DragOperation::ResizeN {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::South => DragOperation::ResizeS {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::East => DragOperation::ResizeE {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::West => DragOperation::ResizeW {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::NorthWest => DragOperation::ResizeNW {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::NorthEast => DragOperation::ResizeNE {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::SouthWest => DragOperation::ResizeSW {
                start: mouse_pos,
                window_start: window_rect,
            },
            ResizeHandle::SouthEast => DragOperation::ResizeSE {
                start: mouse_pos,
                window_start: window_rect,
            },
        };
    }

    /// Update drag operation
    pub fn update_drag(&self, mouse_pos: Point) -> Option<Rect> {
        match self.drag_op {
            DragOperation::None => None,
            DragOperation::Move { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                let dy = mouse_pos.y - start.y;
                let mut new_rect = Rect::new(
                    window_start.x + dx,
                    window_start.y + dy,
                    0, 0, // Width/height unchanged for move
                );

                // Apply snap to edges
                if self.config.snap_to_edges {
                    new_rect = self.snap_rect(new_rect);
                }

                Some(new_rect)
            }
            DragOperation::ResizeNW { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                let dy = mouse_pos.y - start.y;
                Some(self.calculate_resize_nw(window_start, dx, dy))
            }
            DragOperation::ResizeN { start, window_start } => {
                let dy = mouse_pos.y - start.y;
                Some(self.calculate_resize_n(window_start, dy))
            }
            DragOperation::ResizeNE { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                let dy = mouse_pos.y - start.y;
                Some(self.calculate_resize_ne(window_start, dx, dy))
            }
            DragOperation::ResizeE { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                Some(self.calculate_resize_e(window_start, dx))
            }
            DragOperation::ResizeSE { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                let dy = mouse_pos.y - start.y;
                Some(self.calculate_resize_se(window_start, dx, dy))
            }
            DragOperation::ResizeS { start, window_start } => {
                let dy = mouse_pos.y - start.y;
                Some(self.calculate_resize_s(window_start, dy))
            }
            DragOperation::ResizeSW { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                let dy = mouse_pos.y - start.y;
                Some(self.calculate_resize_sw(window_start, dx, dy))
            }
            DragOperation::ResizeW { start, window_start } => {
                let dx = mouse_pos.x - start.x;
                Some(self.calculate_resize_w(window_start, dx))
            }
        }
    }

    /// End drag operation
    pub fn end_drag(&mut self) -> Option<WindowId> {
        self.drag_op = DragOperation::None;
        self.drag_window.take()
    }

    /// Check if dragging
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag_op, DragOperation::None)
    }

    /// Snap rectangle to screen edges
    fn snap_rect(&self, rect: Rect) -> Rect {
        let mut x = rect.x;
        let mut y = rect.y;
        let snap = self.config.snap_distance as i32;

        // Snap to left edge
        if (x - self.work_area.x).abs() < snap {
            x = self.work_area.x;
        }
        // Snap to top edge
        if (y - self.work_area.y).abs() < snap {
            y = self.work_area.y;
        }
        // Snap to right edge
        let right = x + rect.width as i32;
        if (right - self.work_area.right()).abs() < snap {
            x = self.work_area.right() - rect.width as i32;
        }
        // Snap to bottom edge
        let bottom = y + rect.height as i32;
        if (bottom - self.work_area.bottom()).abs() < snap {
            y = self.work_area.bottom() - rect.height as i32;
        }

        Rect::new(x, y, rect.width, rect.height)
    }

    /// Calculate resize from NW corner
    fn calculate_resize_nw(&self, start: Rect, dx: i32, dy: i32) -> Rect {
        let new_width = (start.width as i32 - dx).max(self.config.min_window_width as i32) as u32;
        let new_height = (start.height as i32 - dy).max(self.config.min_window_height as i32) as u32;
        let new_x = start.right() - new_width as i32;
        let new_y = start.bottom() - new_height as i32;
        Rect::new(new_x, new_y, new_width, new_height)
    }

    /// Calculate resize from N edge
    fn calculate_resize_n(&self, start: Rect, dy: i32) -> Rect {
        let new_height = (start.height as i32 - dy).max(self.config.min_window_height as i32) as u32;
        let new_y = start.bottom() - new_height as i32;
        Rect::new(start.x, new_y, start.width, new_height)
    }

    /// Calculate resize from NE corner
    fn calculate_resize_ne(&self, start: Rect, dx: i32, dy: i32) -> Rect {
        let new_width = (start.width as i32 + dx).max(self.config.min_window_width as i32) as u32;
        let new_height = (start.height as i32 - dy).max(self.config.min_window_height as i32) as u32;
        let new_y = start.bottom() - new_height as i32;
        Rect::new(start.x, new_y, new_width, new_height)
    }

    /// Calculate resize from E edge
    fn calculate_resize_e(&self, start: Rect, dx: i32) -> Rect {
        let new_width = (start.width as i32 + dx).max(self.config.min_window_width as i32) as u32;
        Rect::new(start.x, start.y, new_width, start.height)
    }

    /// Calculate resize from SE corner
    fn calculate_resize_se(&self, start: Rect, dx: i32, dy: i32) -> Rect {
        let new_width = (start.width as i32 + dx).max(self.config.min_window_width as i32) as u32;
        let new_height = (start.height as i32 + dy).max(self.config.min_window_height as i32) as u32;
        Rect::new(start.x, start.y, new_width, new_height)
    }

    /// Calculate resize from S edge
    fn calculate_resize_s(&self, start: Rect, dy: i32) -> Rect {
        let new_height = (start.height as i32 + dy).max(self.config.min_window_height as i32) as u32;
        Rect::new(start.x, start.y, start.width, new_height)
    }

    /// Calculate resize from SW corner
    fn calculate_resize_sw(&self, start: Rect, dx: i32, dy: i32) -> Rect {
        let new_width = (start.width as i32 - dx).max(self.config.min_window_width as i32) as u32;
        let new_height = (start.height as i32 + dy).max(self.config.min_window_height as i32) as u32;
        let new_x = start.right() - new_width as i32;
        Rect::new(new_x, start.y, new_width, new_height)
    }

    /// Calculate resize from W edge
    fn calculate_resize_w(&self, start: Rect, dx: i32) -> Rect {
        let new_width = (start.width as i32 - dx).max(self.config.min_window_width as i32) as u32;
        let new_x = start.right() - new_width as i32;
        Rect::new(new_x, start.y, new_width, start.height)
    }

    /// Check hot corners
    pub fn check_hot_corner(&self, pos: Point) -> Option<HotCornerAction> {
        if !self.hot_corners_enabled {
            return None;
        }

        const CORNER_SIZE: i32 = 10;
        let screen_w = self.screen_size.width as i32;
        let screen_h = self.screen_size.height as i32;

        if pos.x < CORNER_SIZE && pos.y < CORNER_SIZE {
            Some(HotCornerAction::TopLeft)
        } else if pos.x > screen_w - CORNER_SIZE && pos.y < CORNER_SIZE {
            Some(HotCornerAction::TopRight)
        } else if pos.x < CORNER_SIZE && pos.y > screen_h - CORNER_SIZE {
            Some(HotCornerAction::BottomLeft)
        } else if pos.x > screen_w - CORNER_SIZE && pos.y > screen_h - CORNER_SIZE {
            Some(HotCornerAction::BottomRight)
        } else {
            None
        }
    }
}

/// Hot corner actions
#[derive(Clone, Copy, Debug)]
pub enum HotCornerAction {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Window group (for virtual desktops)
#[derive(Clone, Debug)]
pub struct WindowGroup {
    /// Group ID
    pub id: u32,
    /// Group name
    pub name: String,
    /// Windows in this group
    pub windows: Vec<WindowId>,
    /// Is active
    pub active: bool,
}

impl WindowGroup {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: String::from(name),
            windows: Vec::new(),
            active: false,
        }
    }

    pub fn add_window(&mut self, id: WindowId) {
        if !self.windows.contains(&id) {
            self.windows.push(id);
        }
    }

    pub fn remove_window(&mut self, id: WindowId) {
        self.windows.retain(|&w| w != id);
    }
}

/// Virtual desktop manager
pub struct VirtualDesktopManager {
    /// Desktop groups
    groups: Vec<WindowGroup>,
    /// Active desktop index
    active_index: usize,
    /// Next group ID
    next_group_id: u32,
}

impl VirtualDesktopManager {
    pub fn new() -> Self {
        let mut manager = Self {
            groups: Vec::new(),
            active_index: 0,
            next_group_id: 1,
        };

        // Create default desktop
        manager.create_desktop("Desktop 1");
        manager.groups[0].active = true;

        manager
    }

    /// Create a new virtual desktop
    pub fn create_desktop(&mut self, name: &str) -> u32 {
        let id = self.next_group_id;
        self.next_group_id += 1;
        self.groups.push(WindowGroup::new(id, name));
        id
    }

    /// Remove a virtual desktop
    pub fn remove_desktop(&mut self, id: u32) -> Option<Vec<WindowId>> {
        if self.groups.len() <= 1 {
            return None; // Can't remove last desktop
        }

        if let Some(idx) = self.groups.iter().position(|g| g.id == id) {
            let group = self.groups.remove(idx);
            if self.active_index >= self.groups.len() {
                self.active_index = self.groups.len() - 1;
            }
            if !self.groups.is_empty() {
                self.groups[self.active_index].active = true;
            }
            Some(group.windows)
        } else {
            None
        }
    }

    /// Switch to desktop
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.groups.len() && index != self.active_index {
            self.groups[self.active_index].active = false;
            self.active_index = index;
            self.groups[self.active_index].active = true;
            true
        } else {
            false
        }
    }

    /// Get active desktop
    pub fn active_desktop(&self) -> Option<&WindowGroup> {
        self.groups.get(self.active_index)
    }

    /// Get active desktop mutable
    pub fn active_desktop_mut(&mut self) -> Option<&mut WindowGroup> {
        self.groups.get_mut(self.active_index)
    }

    /// Move window to desktop
    pub fn move_window_to(&mut self, window_id: WindowId, desktop_id: u32) {
        // Remove from all desktops
        for group in &mut self.groups {
            group.remove_window(window_id);
        }

        // Add to target desktop
        if let Some(group) = self.groups.iter_mut().find(|g| g.id == desktop_id) {
            group.add_window(window_id);
        }
    }

    /// Get number of desktops
    pub fn count(&self) -> usize {
        self.groups.len()
    }

    /// Get desktop at index
    pub fn get(&self, index: usize) -> Option<&WindowGroup> {
        self.groups.get(index)
    }
}

impl Default for VirtualDesktopManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_rect() {
        let wm = WindowManager::new(Size::new(1920, 1080));
        let left = wm.tile_rect(TileMode::Left);
        assert_eq!(left.width, 960);
        assert_eq!(left.x, 0);
    }

    #[test]
    fn test_resize_handle() {
        let window = Rect::new(100, 100, 400, 300);
        assert_eq!(ResizeHandle::at_point(Point::new(100, 100), &window), ResizeHandle::NorthWest);
        assert_eq!(ResizeHandle::at_point(Point::new(300, 250), &window), ResizeHandle::None);
    }

    #[test]
    fn test_virtual_desktops() {
        let mut vdm = VirtualDesktopManager::new();
        assert_eq!(vdm.count(), 1);

        vdm.create_desktop("Desktop 2");
        assert_eq!(vdm.count(), 2);

        assert!(vdm.switch_to(1));
        assert_eq!(vdm.active_index, 1);
    }
}
