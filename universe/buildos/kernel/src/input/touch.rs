//! Touchscreen Input Driver
//!
//! This module implements comprehensive touchscreen support including:
//!
//! - Multi-touch Protocol A (legacy)
//! - Multi-touch Protocol B (slots)
//! - Gesture recognition
//! - Touch pressure and size
//! - Palm rejection
//! - Touch calibration

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use spin::{RwLock, Mutex};

use crate::math::F32Ext;
use super::events::*;
use super::{InputDevice, InputId, InputHandler, InputError, InputEvent, MtState, input_manager};

/// Maximum number of simultaneous touches
pub const MAX_TOUCH_POINTS: usize = 10;

/// Touch point state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TouchState {
    /// Touch point is not active
    #[default]
    Up,
    /// Touch point just touched down
    Down,
    /// Touch point is moving
    Move,
    /// Touch point just lifted
    Release,
}

/// Touch point data
#[derive(Debug, Clone, Copy, Default)]
pub struct TouchPoint {
    /// Tracking ID (-1 = inactive)
    pub tracking_id: i32,
    /// X coordinate
    pub x: i32,
    /// Y coordinate
    pub y: i32,
    /// Touch pressure (0-255 typically)
    pub pressure: i32,
    /// Touch major axis (size)
    pub touch_major: i32,
    /// Touch minor axis
    pub touch_minor: i32,
    /// Orientation angle
    pub orientation: i32,
    /// Tool type (finger, pen, etc.)
    pub tool_type: TouchToolType,
    /// Touch state
    pub state: TouchState,
    /// Timestamp (microseconds)
    pub timestamp: u64,
}

impl TouchPoint {
    pub const fn new() -> Self {
        Self {
            tracking_id: -1,
            x: 0,
            y: 0,
            pressure: 0,
            touch_major: 0,
            touch_minor: 0,
            orientation: 0,
            tool_type: TouchToolType::Finger,
            state: TouchState::Up,
            timestamp: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.tracking_id >= 0
    }

    /// Calculate distance to another point
    pub fn distance_to(&self, other: &TouchPoint) -> f32 {
        let dx = (self.x - other.x) as f32;
        let dy = (self.y - other.y) as f32;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Touch tool type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TouchToolType {
    #[default]
    Finger,
    Pen,
    Palm,
    Unknown,
}

impl From<u16> for TouchToolType {
    fn from(value: u16) -> Self {
        match value {
            MT_TOOL_FINGER => TouchToolType::Finger,
            MT_TOOL_PEN => TouchToolType::Pen,
            MT_TOOL_PALM => TouchToolType::Palm,
            _ => TouchToolType::Unknown,
        }
    }
}

/// Touch frame (snapshot of all touches at one time)
#[derive(Debug, Clone)]
pub struct TouchFrame {
    /// Touch points
    pub points: [TouchPoint; MAX_TOUCH_POINTS],
    /// Number of active touches
    pub touch_count: usize,
    /// Timestamp
    pub timestamp: u64,
}

impl TouchFrame {
    pub fn new() -> Self {
        Self {
            points: [TouchPoint::new(); MAX_TOUCH_POINTS],
            touch_count: 0,
            timestamp: 0,
        }
    }

    /// Get active touch points
    pub fn active_points(&self) -> impl Iterator<Item = &TouchPoint> {
        self.points.iter().filter(|p| p.is_active())
    }

    /// Find touch by tracking ID
    pub fn find_by_id(&self, tracking_id: i32) -> Option<&TouchPoint> {
        self.points.iter().find(|p| p.tracking_id == tracking_id)
    }

    /// Get primary touch (usually first finger)
    pub fn primary(&self) -> Option<&TouchPoint> {
        self.points.iter().find(|p| p.is_active())
    }
}

impl Default for TouchFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// Gesture type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureType {
    /// No gesture detected
    None,
    /// Single tap
    Tap,
    /// Double tap
    DoubleTap,
    /// Long press
    LongPress,
    /// Swipe in a direction
    Swipe(SwipeDirection),
    /// Two-finger pinch (scale factor)
    Pinch,
    /// Two-finger rotate
    Rotate,
    /// Two-finger scroll
    Scroll,
    /// Three-finger swipe
    ThreeFingerSwipe(SwipeDirection),
    /// Four-finger swipe
    FourFingerSwipe(SwipeDirection),
}

/// Swipe direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Gesture state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureState {
    /// Gesture started
    Begin,
    /// Gesture in progress
    Update,
    /// Gesture ended
    End,
    /// Gesture cancelled
    Cancel,
}

/// Gesture data
#[derive(Debug, Clone, Copy)]
pub struct Gesture {
    /// Gesture type
    pub gesture_type: GestureType,
    /// Gesture state
    pub state: GestureState,
    /// Center X position
    pub x: i32,
    /// Center Y position
    pub y: i32,
    /// Delta X (for swipes/scrolls)
    pub dx: i32,
    /// Delta Y
    pub dy: i32,
    /// Scale factor (for pinch)
    pub scale: f32,
    /// Rotation angle (for rotate)
    pub rotation: f32,
    /// Velocity (for swipes)
    pub velocity: f32,
}

impl Gesture {
    pub const fn new(gesture_type: GestureType, state: GestureState) -> Self {
        Self {
            gesture_type,
            state,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
            scale: 1.0,
            rotation: 0.0,
            velocity: 0.0,
        }
    }
}

/// Gesture recognizer configuration
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    /// Tap maximum duration (ms)
    pub tap_timeout: u32,
    /// Double tap maximum interval (ms)
    pub double_tap_timeout: u32,
    /// Long press minimum duration (ms)
    pub long_press_timeout: u32,
    /// Minimum swipe distance (pixels)
    pub swipe_min_distance: i32,
    /// Minimum swipe velocity (pixels/second)
    pub swipe_min_velocity: f32,
    /// Pinch minimum distance change (pixels)
    pub pinch_min_distance: i32,
    /// Rotation minimum angle change (radians)
    pub rotate_min_angle: f32,
    /// Maximum tap movement (pixels)
    pub tap_slop: i32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            tap_timeout: 200,
            double_tap_timeout: 300,
            long_press_timeout: 500,
            swipe_min_distance: 50,
            swipe_min_velocity: 200.0,
            pinch_min_distance: 20,
            rotate_min_angle: 0.1,
            tap_slop: 10,
        }
    }
}

/// Gesture recognizer
pub struct GestureRecognizer {
    /// Configuration
    config: GestureConfig,
    /// Previous frame
    prev_frame: TouchFrame,
    /// Initial touch positions (for gestures)
    initial_points: [TouchPoint; MAX_TOUCH_POINTS],
    /// Initial distance (for pinch)
    initial_distance: f32,
    /// Initial angle (for rotate)
    initial_angle: f32,
    /// Touch start time
    touch_start_time: u64,
    /// Last tap time (for double tap detection)
    last_tap_time: u64,
    /// Last tap position
    last_tap_x: i32,
    last_tap_y: i32,
    /// Gesture in progress
    active_gesture: Option<GestureType>,
    /// Long press pending
    long_press_pending: bool,
}

impl GestureRecognizer {
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            prev_frame: TouchFrame::new(),
            initial_points: [TouchPoint::new(); MAX_TOUCH_POINTS],
            initial_distance: 0.0,
            initial_angle: 0.0,
            touch_start_time: 0,
            last_tap_time: 0,
            last_tap_x: 0,
            last_tap_y: 0,
            active_gesture: None,
            long_press_pending: false,
        }
    }

    /// Process a touch frame and detect gestures
    pub fn process(&mut self, frame: &TouchFrame) -> Option<Gesture> {
        let prev_count = self.prev_frame.touch_count;
        let curr_count = frame.touch_count;

        let gesture = if prev_count == 0 && curr_count > 0 {
            // Touch down
            self.on_touch_down(frame)
        } else if prev_count > 0 && curr_count == 0 {
            // Touch up
            self.on_touch_up(frame)
        } else if curr_count > 0 {
            // Touch move
            self.on_touch_move(frame)
        } else {
            None
        };

        self.prev_frame = frame.clone();
        gesture
    }

    /// Handle touch down
    fn on_touch_down(&mut self, frame: &TouchFrame) -> Option<Gesture> {
        self.touch_start_time = frame.timestamp;
        self.long_press_pending = true;

        // Store initial positions
        for (i, point) in frame.points.iter().enumerate() {
            self.initial_points[i] = *point;
        }

        // Calculate initial distance for pinch
        if frame.touch_count >= 2 {
            if let (Some(p1), Some(p2)) = (frame.points.get(0), frame.points.get(1)) {
                if p1.is_active() && p2.is_active() {
                    self.initial_distance = p1.distance_to(p2);
                    self.initial_angle = self.angle_between(p1, p2);
                }
            }
        }

        None
    }

    /// Handle touch up
    fn on_touch_up(&mut self, frame: &TouchFrame) -> Option<Gesture> {
        self.long_press_pending = false;

        let duration = frame.timestamp.saturating_sub(self.touch_start_time) / 1000; // to ms

        // Check for tap
        if duration < self.config.tap_timeout as u64 {
            if let Some(initial) = self.initial_points.iter().find(|p| p.is_active()) {
                let moved_x = (initial.x - self.prev_frame.points[0].x).abs();
                let moved_y = (initial.y - self.prev_frame.points[0].y).abs();

                if moved_x < self.config.tap_slop && moved_y < self.config.tap_slop {
                    // Check for double tap
                    let time_since_last_tap = frame.timestamp.saturating_sub(self.last_tap_time) / 1000;
                    let dist_from_last_tap_x = (initial.x - self.last_tap_x).abs();
                    let dist_from_last_tap_y = (initial.y - self.last_tap_y).abs();

                    if time_since_last_tap < self.config.double_tap_timeout as u64 &&
                       dist_from_last_tap_x < self.config.tap_slop * 2 &&
                       dist_from_last_tap_y < self.config.tap_slop * 2 {
                        self.last_tap_time = 0;
                        return Some(Gesture {
                            gesture_type: GestureType::DoubleTap,
                            state: GestureState::End,
                            x: initial.x,
                            y: initial.y,
                            ..Gesture::new(GestureType::DoubleTap, GestureState::End)
                        });
                    }

                    self.last_tap_time = frame.timestamp;
                    self.last_tap_x = initial.x;
                    self.last_tap_y = initial.y;

                    return Some(Gesture {
                        gesture_type: GestureType::Tap,
                        state: GestureState::End,
                        x: initial.x,
                        y: initial.y,
                        ..Gesture::new(GestureType::Tap, GestureState::End)
                    });
                }
            }
        }

        // Check for swipe
        if let Some(initial) = self.initial_points.iter().find(|p| p.is_active()) {
            let dx = self.prev_frame.points[0].x - initial.x;
            let dy = self.prev_frame.points[0].y - initial.y;
            let distance = ((dx * dx + dy * dy) as f32).sqrt();

            if distance >= self.config.swipe_min_distance as f32 {
                let direction = self.swipe_direction(dx, dy);
                let velocity = distance / (duration as f32 / 1000.0);

                if velocity >= self.config.swipe_min_velocity {
                    let gesture_type = match self.prev_frame.touch_count {
                        3 => GestureType::ThreeFingerSwipe(direction),
                        4 => GestureType::FourFingerSwipe(direction),
                        _ => GestureType::Swipe(direction),
                    };

                    return Some(Gesture {
                        gesture_type,
                        state: GestureState::End,
                        x: initial.x,
                        y: initial.y,
                        dx,
                        dy,
                        velocity,
                        ..Gesture::new(gesture_type, GestureState::End)
                    });
                }
            }
        }

        // End any active gesture
        if let Some(gesture_type) = self.active_gesture.take() {
            return Some(Gesture::new(gesture_type, GestureState::End));
        }

        None
    }

    /// Handle touch move
    fn on_touch_move(&mut self, frame: &TouchFrame) -> Option<Gesture> {
        // Check for long press
        if self.long_press_pending {
            let duration = frame.timestamp.saturating_sub(self.touch_start_time) / 1000;
            if duration >= self.config.long_press_timeout as u64 {
                self.long_press_pending = false;

                if let Some(point) = frame.primary() {
                    return Some(Gesture {
                        gesture_type: GestureType::LongPress,
                        state: GestureState::Begin,
                        x: point.x,
                        y: point.y,
                        ..Gesture::new(GestureType::LongPress, GestureState::Begin)
                    });
                }
            }

            // Cancel long press if moved too much
            if let Some(initial) = self.initial_points.iter().find(|p| p.is_active()) {
                if let Some(current) = frame.primary() {
                    let dx = (current.x - initial.x).abs();
                    let dy = (current.y - initial.y).abs();
                    if dx > self.config.tap_slop || dy > self.config.tap_slop {
                        self.long_press_pending = false;
                    }
                }
            }
        }

        // Two-finger gestures
        if frame.touch_count >= 2 {
            let points: Vec<_> = frame.active_points().take(2).collect();
            if points.len() == 2 {
                let current_distance = points[0].distance_to(points[1]);
                let current_angle = self.angle_between(points[0], points[1]);

                // Pinch detection
                let distance_delta = current_distance - self.initial_distance;
                if distance_delta.abs() >= self.config.pinch_min_distance as f32 {
                    let scale = current_distance / self.initial_distance;
                    let center_x = (points[0].x + points[1].x) / 2;
                    let center_y = (points[0].y + points[1].y) / 2;

                    let state = if self.active_gesture == Some(GestureType::Pinch) {
                        GestureState::Update
                    } else {
                        self.active_gesture = Some(GestureType::Pinch);
                        GestureState::Begin
                    };

                    return Some(Gesture {
                        gesture_type: GestureType::Pinch,
                        state,
                        x: center_x,
                        y: center_y,
                        scale,
                        ..Gesture::new(GestureType::Pinch, state)
                    });
                }

                // Rotate detection
                let angle_delta = current_angle - self.initial_angle;
                if angle_delta.abs() >= self.config.rotate_min_angle {
                    let center_x = (points[0].x + points[1].x) / 2;
                    let center_y = (points[0].y + points[1].y) / 2;

                    let state = if self.active_gesture == Some(GestureType::Rotate) {
                        GestureState::Update
                    } else {
                        self.active_gesture = Some(GestureType::Rotate);
                        GestureState::Begin
                    };

                    return Some(Gesture {
                        gesture_type: GestureType::Rotate,
                        state,
                        x: center_x,
                        y: center_y,
                        rotation: angle_delta,
                        ..Gesture::new(GestureType::Rotate, state)
                    });
                }

                // Two-finger scroll
                if let Some(prev_p1) = self.prev_frame.find_by_id(points[0].tracking_id) {
                    if let Some(prev_p2) = self.prev_frame.find_by_id(points[1].tracking_id) {
                        let prev_center_x = (prev_p1.x + prev_p2.x) / 2;
                        let prev_center_y = (prev_p1.y + prev_p2.y) / 2;
                        let curr_center_x = (points[0].x + points[1].x) / 2;
                        let curr_center_y = (points[0].y + points[1].y) / 2;

                        let dx = curr_center_x - prev_center_x;
                        let dy = curr_center_y - prev_center_y;

                        if dx != 0 || dy != 0 {
                            let state = if self.active_gesture == Some(GestureType::Scroll) {
                                GestureState::Update
                            } else {
                                self.active_gesture = Some(GestureType::Scroll);
                                GestureState::Begin
                            };

                            return Some(Gesture {
                                gesture_type: GestureType::Scroll,
                                state,
                                x: curr_center_x,
                                y: curr_center_y,
                                dx,
                                dy,
                                ..Gesture::new(GestureType::Scroll, state)
                            });
                        }
                    }
                }
            }
        }

        None
    }

    /// Calculate angle between two points
    fn angle_between(&self, p1: &TouchPoint, p2: &TouchPoint) -> f32 {
        let dx = (p2.x - p1.x) as f32;
        let dy = (p2.y - p1.y) as f32;
        libm::atan2f(dy, dx)
    }

    /// Determine swipe direction
    fn swipe_direction(&self, dx: i32, dy: i32) -> SwipeDirection {
        if dx.abs() > dy.abs() {
            if dx > 0 { SwipeDirection::Right } else { SwipeDirection::Left }
        } else {
            if dy > 0 { SwipeDirection::Down } else { SwipeDirection::Up }
        }
    }

    /// Check for long press timeout
    pub fn check_long_press(&mut self, current_time: u64) -> Option<Gesture> {
        if self.long_press_pending && self.prev_frame.touch_count > 0 {
            let duration = current_time.saturating_sub(self.touch_start_time) / 1000;
            if duration >= self.config.long_press_timeout as u64 {
                self.long_press_pending = false;

                if let Some(point) = self.prev_frame.primary() {
                    return Some(Gesture {
                        gesture_type: GestureType::LongPress,
                        state: GestureState::Begin,
                        x: point.x,
                        y: point.y,
                        ..Gesture::new(GestureType::LongPress, GestureState::Begin)
                    });
                }
            }
        }
        None
    }
}

/// Touch calibration data
#[derive(Debug, Clone, Copy)]
pub struct TouchCalibration {
    /// X offset
    pub x_offset: i32,
    /// Y offset
    pub y_offset: i32,
    /// X scale (multiplied by 1000)
    pub x_scale: i32,
    /// Y scale (multiplied by 1000)
    pub y_scale: i32,
    /// Swap X and Y axes
    pub swap_xy: bool,
    /// Invert X axis
    pub invert_x: bool,
    /// Invert Y axis
    pub invert_y: bool,
}

impl Default for TouchCalibration {
    fn default() -> Self {
        Self {
            x_offset: 0,
            y_offset: 0,
            x_scale: 1000,
            y_scale: 1000,
            swap_xy: false,
            invert_x: false,
            invert_y: false,
        }
    }
}

impl TouchCalibration {
    /// Apply calibration to coordinates
    pub fn apply(&self, mut x: i32, mut y: i32, width: i32, height: i32) -> (i32, i32) {
        // Apply offset
        x += self.x_offset;
        y += self.y_offset;

        // Apply scale
        x = (x * self.x_scale) / 1000;
        y = (y * self.y_scale) / 1000;

        // Swap axes
        if self.swap_xy {
            core::mem::swap(&mut x, &mut y);
        }

        // Invert axes
        if self.invert_x {
            x = width - 1 - x;
        }
        if self.invert_y {
            y = height - 1 - y;
        }

        (x.clamp(0, width - 1), y.clamp(0, height - 1))
    }
}

/// Touchscreen driver
pub struct TouchscreenDriver {
    /// Input device
    device: Arc<InputDevice>,
    /// Multi-touch state
    mt_state: Mutex<MtState>,
    /// Current touch frame
    current_frame: RwLock<TouchFrame>,
    /// Gesture recognizer
    gesture_recognizer: Mutex<GestureRecognizer>,
    /// Calibration data
    calibration: RwLock<TouchCalibration>,
    /// Screen dimensions
    screen_width: AtomicI32,
    screen_height: AtomicI32,
    /// Touch resolution
    touch_max_x: AtomicI32,
    touch_max_y: AtomicI32,
    /// Gestures enabled
    gestures_enabled: AtomicBool,
    /// Palm rejection enabled
    palm_rejection: AtomicBool,
    /// Palm rejection threshold (touch size)
    palm_threshold: AtomicI32,
}

impl TouchscreenDriver {
    pub fn new(name: &str, vendor_id: u16, product_id: u16, width: i32, height: i32) -> Self {
        let device = InputDevice::touchscreen(name, InputId::usb(vendor_id, product_id, 1), width, height);

        Self {
            device: Arc::new(device),
            mt_state: Mutex::new(MtState::new()),
            current_frame: RwLock::new(TouchFrame::new()),
            gesture_recognizer: Mutex::new(GestureRecognizer::new(GestureConfig::default())),
            calibration: RwLock::new(TouchCalibration::default()),
            screen_width: AtomicI32::new(width),
            screen_height: AtomicI32::new(height),
            touch_max_x: AtomicI32::new(width),
            touch_max_y: AtomicI32::new(height),
            gestures_enabled: AtomicBool::new(true),
            palm_rejection: AtomicBool::new(true),
            palm_threshold: AtomicI32::new(200),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        input_manager().register_device(self.device.clone())
    }

    /// Set screen dimensions
    pub fn set_screen_size(&self, width: i32, height: i32) {
        self.screen_width.store(width, Ordering::Relaxed);
        self.screen_height.store(height, Ordering::Relaxed);
    }

    /// Set touch resolution
    pub fn set_touch_resolution(&self, max_x: i32, max_y: i32) {
        self.touch_max_x.store(max_x, Ordering::Relaxed);
        self.touch_max_y.store(max_y, Ordering::Relaxed);
    }

    /// Set calibration
    pub fn set_calibration(&self, calibration: TouchCalibration) {
        *self.calibration.write() = calibration;
    }

    /// Process multi-touch event
    pub fn process_mt_event(&self, code: u16, value: i32) {
        let mut mt = self.mt_state.lock();
        mt.process_event(code, value);
    }

    /// Process sync event (end of touch report)
    pub fn process_sync(&self, timestamp: u64) {
        let mt = self.mt_state.lock();
        let mut frame = self.current_frame.write();

        // Update frame from MT state
        frame.timestamp = timestamp;
        frame.touch_count = mt.touch_count();

        for (i, slot) in mt.slots().iter().enumerate() {
            if i >= MAX_TOUCH_POINTS {
                break;
            }

            frame.points[i] = if slot.is_active() {
                // Apply calibration
                let (x, y) = self.transform_coordinates(slot.x, slot.y);

                // Check palm rejection
                if self.palm_rejection.load(Ordering::Relaxed) {
                    let threshold = self.palm_threshold.load(Ordering::Relaxed);
                    if slot.touch_major > threshold || slot.touch_minor > threshold {
                        // Reject as palm
                        TouchPoint::new()
                    } else {
                        TouchPoint {
                            tracking_id: slot.tracking_id,
                            x,
                            y,
                            pressure: slot.pressure,
                            touch_major: slot.touch_major,
                            touch_minor: slot.touch_minor,
                            orientation: slot.orientation,
                            tool_type: TouchToolType::Finger,
                            state: TouchState::Move,
                            timestamp,
                        }
                    }
                } else {
                    TouchPoint {
                        tracking_id: slot.tracking_id,
                        x,
                        y,
                        pressure: slot.pressure,
                        touch_major: slot.touch_major,
                        touch_minor: slot.touch_minor,
                        orientation: slot.orientation,
                        tool_type: TouchToolType::Finger,
                        state: TouchState::Move,
                        timestamp,
                    }
                }
            } else {
                TouchPoint::new()
            };
        }

        // Process gestures
        if self.gestures_enabled.load(Ordering::Relaxed) {
            let frame_clone = frame.clone();
            drop(frame);

            let mut recognizer = self.gesture_recognizer.lock();
            if let Some(_gesture) = recognizer.process(&frame_clone) {
                // Dispatch gesture event
                // This would be sent to a gesture handler
            }
        }
    }

    /// Transform touch coordinates to screen coordinates
    fn transform_coordinates(&self, x: i32, y: i32) -> (i32, i32) {
        let touch_max_x = self.touch_max_x.load(Ordering::Relaxed);
        let touch_max_y = self.touch_max_y.load(Ordering::Relaxed);
        let screen_width = self.screen_width.load(Ordering::Relaxed);
        let screen_height = self.screen_height.load(Ordering::Relaxed);

        // Scale to screen coordinates
        let screen_x = (x * screen_width) / touch_max_x.max(1);
        let screen_y = (y * screen_height) / touch_max_y.max(1);

        // Apply calibration
        self.calibration.read().apply(screen_x, screen_y, screen_width, screen_height)
    }

    /// Get current touch frame
    pub fn get_frame(&self) -> TouchFrame {
        self.current_frame.read().clone()
    }

    /// Get primary touch position
    pub fn get_primary_position(&self) -> Option<(i32, i32)> {
        let frame = self.current_frame.read();
        frame.primary().map(|p| (p.x, p.y))
    }

    /// Get touch count
    pub fn touch_count(&self) -> usize {
        self.current_frame.read().touch_count
    }

    /// Enable/disable gestures
    pub fn set_gestures_enabled(&self, enabled: bool) {
        self.gestures_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Enable/disable palm rejection
    pub fn set_palm_rejection(&self, enabled: bool) {
        self.palm_rejection.store(enabled, Ordering::Relaxed);
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }
}

/// Touch input handler
pub struct TouchHandler {
    /// Handler name
    name: String,
    /// Gesture callback
    gesture_callback: RwLock<Option<Box<dyn Fn(&Gesture) + Send + Sync>>>,
    /// Touch callback
    touch_callback: RwLock<Option<Box<dyn Fn(&TouchFrame) + Send + Sync>>>,
}

impl TouchHandler {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            gesture_callback: RwLock::new(None),
            touch_callback: RwLock::new(None),
        }
    }

    /// Set gesture callback
    pub fn set_gesture_callback<F>(&self, callback: F)
    where
        F: Fn(&Gesture) + Send + Sync + 'static,
    {
        *self.gesture_callback.write() = Some(Box::new(callback));
    }

    /// Set touch callback
    pub fn set_touch_callback<F>(&self, callback: F)
    where
        F: Fn(&TouchFrame) + Send + Sync + 'static,
    {
        *self.touch_callback.write() = Some(Box::new(callback));
    }
}

impl InputHandler for TouchHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn match_device(&self, device: &InputDevice) -> bool {
        let caps = device.capabilities.read();
        caps.has_evbit(EV_ABS) && caps.has_keybit(BTN_TOUCH)
    }

    fn connect(&self, _device: &InputDevice) -> Result<(), InputError> {
        Ok(())
    }

    fn disconnect(&self, _device: &InputDevice) {}

    fn event(&self, _device: &InputDevice, _event: &InputEvent) {
        // Process touch events here
    }
}

/// Virtual touchscreen for testing
pub struct VirtualTouchscreen {
    /// Touchscreen driver
    driver: TouchscreenDriver,
    /// Next tracking ID
    next_tracking_id: AtomicI32,
}

impl VirtualTouchscreen {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            driver: TouchscreenDriver::new("Virtual Touchscreen", 0, 0, width, height),
            next_tracking_id: AtomicI32::new(0),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        self.driver.register()
    }

    /// Touch down at position
    pub fn touch_down(&self, slot: u8, x: i32, y: i32) -> i32 {
        let tracking_id = self.next_tracking_id.fetch_add(1, Ordering::SeqCst);

        self.driver.device.report_abs(ABS_MT_SLOT, slot as i32);
        self.driver.device.report_abs(ABS_MT_TRACKING_ID, tracking_id);
        self.driver.device.report_abs(ABS_MT_POSITION_X, x);
        self.driver.device.report_abs(ABS_MT_POSITION_Y, y);
        self.driver.device.report_key(BTN_TOUCH, true);
        self.driver.device.sync();

        tracking_id
    }

    /// Touch move
    pub fn touch_move(&self, slot: u8, x: i32, y: i32) {
        self.driver.device.report_abs(ABS_MT_SLOT, slot as i32);
        self.driver.device.report_abs(ABS_MT_POSITION_X, x);
        self.driver.device.report_abs(ABS_MT_POSITION_Y, y);
        self.driver.device.sync();
    }

    /// Touch up
    pub fn touch_up(&self, slot: u8) {
        self.driver.device.report_abs(ABS_MT_SLOT, slot as i32);
        self.driver.device.report_abs(ABS_MT_TRACKING_ID, -1);
        self.driver.device.report_key(BTN_TOUCH, false);
        self.driver.device.sync();
    }

    /// Simulate tap
    pub fn tap(&self, x: i32, y: i32) {
        self.touch_down(0, x, y);
        self.touch_up(0);
    }

    /// Simulate swipe
    pub fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32, steps: u32) {
        self.touch_down(0, x1, y1);

        for i in 1..steps {
            let t = i as f32 / steps as f32;
            let x = x1 + ((x2 - x1) as f32 * t) as i32;
            let y = y1 + ((y2 - y1) as f32 * t) as i32;
            self.touch_move(0, x, y);
        }

        self.touch_move(0, x2, y2);
        self.touch_up(0);
    }

    /// Simulate pinch
    pub fn pinch(&self, cx: i32, cy: i32, start_dist: i32, end_dist: i32, steps: u32) {
        self.touch_down(0, cx - start_dist / 2, cy);
        self.touch_down(1, cx + start_dist / 2, cy);

        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let dist = start_dist + ((end_dist - start_dist) as f32 * t) as i32;
            self.touch_move(0, cx - dist / 2, cy);
            self.touch_move(1, cx + dist / 2, cy);
        }

        self.touch_up(0);
        self.touch_up(1);
    }

    /// Get driver
    pub fn driver(&self) -> &TouchscreenDriver {
        &self.driver
    }
}

/// Initialize touchscreen subsystem
pub fn init() {
    // Touchscreen devices are initialized when detected via USB/I2C enumeration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_touch_point() {
        let mut point = TouchPoint::new();
        assert!(!point.is_active());

        point.tracking_id = 0;
        assert!(point.is_active());
    }

    #[test]
    fn test_touch_frame() {
        let mut frame = TouchFrame::new();
        frame.points[0].tracking_id = 1;
        frame.points[0].x = 100;
        frame.points[0].y = 200;
        frame.touch_count = 1;

        assert_eq!(frame.primary().unwrap().x, 100);
        assert_eq!(frame.find_by_id(1).unwrap().y, 200);
    }

    #[test]
    fn test_calibration() {
        let cal = TouchCalibration {
            x_offset: 10,
            y_offset: 20,
            x_scale: 1000,
            y_scale: 1000,
            swap_xy: false,
            invert_x: false,
            invert_y: false,
        };

        let (x, y) = cal.apply(100, 200, 1920, 1080);
        assert_eq!(x, 110);
        assert_eq!(y, 220);
    }

    #[test]
    fn test_swipe_direction() {
        let recognizer = GestureRecognizer::new(GestureConfig::default());

        assert_eq!(recognizer.swipe_direction(100, 10), SwipeDirection::Right);
        assert_eq!(recognizer.swipe_direction(-100, 10), SwipeDirection::Left);
        assert_eq!(recognizer.swipe_direction(10, 100), SwipeDirection::Down);
        assert_eq!(recognizer.swipe_direction(10, -100), SwipeDirection::Up);
    }
}
