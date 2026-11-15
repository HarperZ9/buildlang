//! Force Feedback Subsystem
//!
//! This module implements force feedback (haptic feedback) support including:
//!
//! - Rumble motors (gamepads)
//! - Periodic effects (sine, square, triangle waves)
//! - Constant force effects
//! - Spring/damper/friction effects (steering wheels)
//! - Ramp effects
//! - Custom waveforms
//!
//! Compatible with the Linux Force Feedback API.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI16, AtomicU16, Ordering};
use spin::RwLock;

use super::events::*;
use super::InputError;

/// Maximum number of concurrent effects per device
pub const FF_MAX_EFFECTS: usize = 16;

/// Force feedback effect type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfEffectType {
    /// Simple rumble effect (left/right motors)
    Rumble,
    /// Periodic waveform (sine, square, triangle, etc.)
    Periodic,
    /// Constant force in a direction
    Constant,
    /// Spring effect (resistance increases with displacement)
    Spring,
    /// Friction effect (resistance to movement)
    Friction,
    /// Damper effect (resistance proportional to velocity)
    Damper,
    /// Inertia effect (resistance to acceleration)
    Inertia,
    /// Ramp effect (force changes over time)
    Ramp,
}

impl FfEffectType {
    /// Convert to evdev code
    pub fn to_evdev_code(&self) -> u16 {
        match self {
            FfEffectType::Rumble => FF_RUMBLE,
            FfEffectType::Periodic => FF_PERIODIC,
            FfEffectType::Constant => FF_CONSTANT,
            FfEffectType::Spring => FF_SPRING,
            FfEffectType::Friction => FF_FRICTION,
            FfEffectType::Damper => FF_DAMPER,
            FfEffectType::Inertia => FF_INERTIA,
            FfEffectType::Ramp => FF_RAMP,
        }
    }

    /// Create from evdev code
    pub fn from_evdev_code(code: u16) -> Option<Self> {
        Some(match code {
            FF_RUMBLE => FfEffectType::Rumble,
            FF_PERIODIC => FfEffectType::Periodic,
            FF_CONSTANT => FfEffectType::Constant,
            FF_SPRING => FfEffectType::Spring,
            FF_FRICTION => FfEffectType::Friction,
            FF_DAMPER => FfEffectType::Damper,
            FF_INERTIA => FfEffectType::Inertia,
            FF_RAMP => FfEffectType::Ramp,
            _ => return None,
        })
    }
}

/// Periodic waveform type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfWaveform {
    /// Square wave
    Square,
    /// Triangle wave
    Triangle,
    /// Sine wave
    Sine,
    /// Sawtooth up
    SawUp,
    /// Sawtooth down
    SawDown,
    /// Custom waveform
    Custom,
}

impl FfWaveform {
    /// Convert to evdev code
    pub fn to_evdev_code(&self) -> u16 {
        match self {
            FfWaveform::Square => FF_SQUARE,
            FfWaveform::Triangle => FF_TRIANGLE,
            FfWaveform::Sine => FF_SINE,
            FfWaveform::SawUp => FF_SAW_UP,
            FfWaveform::SawDown => FF_SAW_DOWN,
            FfWaveform::Custom => FF_CUSTOM,
        }
    }

    /// Create from evdev code
    pub fn from_evdev_code(code: u16) -> Option<Self> {
        Some(match code {
            FF_SQUARE => FfWaveform::Square,
            FF_TRIANGLE => FfWaveform::Triangle,
            FF_SINE => FfWaveform::Sine,
            FF_SAW_UP => FfWaveform::SawUp,
            FF_SAW_DOWN => FfWaveform::SawDown,
            FF_CUSTOM => FfWaveform::Custom,
            _ => return None,
        })
    }

    /// Generate waveform value at phase (0.0 to 1.0)
    pub fn sample(&self, phase: f32) -> f32 {
        match self {
            FfWaveform::Square => {
                if phase < 0.5 { 1.0 } else { -1.0 }
            }
            FfWaveform::Triangle => {
                if phase < 0.5 {
                    phase * 4.0 - 1.0
                } else {
                    3.0 - phase * 4.0
                }
            }
            FfWaveform::Sine => {
                libm::sinf(phase * 2.0 * core::f32::consts::PI)
            }
            FfWaveform::SawUp => {
                phase * 2.0 - 1.0
            }
            FfWaveform::SawDown => {
                1.0 - phase * 2.0
            }
            FfWaveform::Custom => 0.0, // Custom requires user data
        }
    }
}

/// Effect direction (for directional effects)
#[derive(Debug, Clone, Copy)]
pub struct FfDirection {
    /// Direction in 1/100 degrees (0 = down, 9000 = left, 18000 = up, 27000 = right)
    pub direction: u16,
}

impl FfDirection {
    pub const fn new(direction: u16) -> Self {
        Self { direction }
    }

    pub const fn down() -> Self {
        Self { direction: 0 }
    }

    pub const fn left() -> Self {
        Self { direction: 9000 }
    }

    pub const fn up() -> Self {
        Self { direction: 18000 }
    }

    pub const fn right() -> Self {
        Self { direction: 27000 }
    }

    /// Get direction in radians
    pub fn to_radians(&self) -> f32 {
        (self.direction as f32 / 100.0).to_radians()
    }

    /// Get X and Y components (-1.0 to 1.0)
    pub fn components(&self) -> (f32, f32) {
        let radians = self.to_radians();
        (libm::sinf(radians), libm::cosf(radians))
    }
}

impl Default for FfDirection {
    fn default() -> Self {
        Self::down()
    }
}

/// Effect replay parameters
#[derive(Debug, Clone, Copy)]
pub struct FfReplay {
    /// Duration in milliseconds (0 = infinite)
    pub duration: u16,
    /// Delay before effect starts (milliseconds)
    pub delay: u16,
}

impl FfReplay {
    pub const fn new(duration: u16, delay: u16) -> Self {
        Self { duration, delay }
    }

    pub const fn infinite() -> Self {
        Self { duration: 0, delay: 0 }
    }
}

impl Default for FfReplay {
    fn default() -> Self {
        Self::new(1000, 0) // 1 second, no delay
    }
}

/// Effect trigger parameters
#[derive(Debug, Clone, Copy)]
pub struct FfTrigger {
    /// Button to trigger effect
    pub button: u16,
    /// Minimum interval between triggers (milliseconds)
    pub interval: u16,
}

impl FfTrigger {
    pub const fn new(button: u16, interval: u16) -> Self {
        Self { button, interval }
    }

    pub const fn none() -> Self {
        Self { button: 0, interval: 0 }
    }
}

impl Default for FfTrigger {
    fn default() -> Self {
        Self::none()
    }
}

/// Effect envelope (attack/fade)
#[derive(Debug, Clone, Copy)]
pub struct FfEnvelope {
    /// Attack duration (milliseconds)
    pub attack_length: u16,
    /// Attack level (0-65535)
    pub attack_level: u16,
    /// Fade duration (milliseconds)
    pub fade_length: u16,
    /// Fade level (0-65535)
    pub fade_level: u16,
}

impl FfEnvelope {
    pub const fn new() -> Self {
        Self {
            attack_length: 0,
            attack_level: 0,
            fade_length: 0,
            fade_level: 0,
        }
    }

    /// Create with attack only
    pub const fn with_attack(length: u16, level: u16) -> Self {
        Self {
            attack_length: length,
            attack_level: level,
            fade_length: 0,
            fade_level: 0,
        }
    }

    /// Create with fade only
    pub const fn with_fade(length: u16, level: u16) -> Self {
        Self {
            attack_length: 0,
            attack_level: 0,
            fade_length: length,
            fade_level: level,
        }
    }

    /// Calculate envelope multiplier at time t (in ms) for effect of given duration
    pub fn multiplier(&self, t: u16, duration: u16) -> f32 {
        if t < self.attack_length {
            // Attack phase
            let progress = t as f32 / self.attack_length as f32;
            let attack = self.attack_level as f32 / 65535.0;
            attack + (1.0 - attack) * progress
        } else if duration > 0 && t > duration - self.fade_length {
            // Fade phase
            let fade_start = duration - self.fade_length;
            let progress = (t - fade_start) as f32 / self.fade_length as f32;
            let fade = self.fade_level as f32 / 65535.0;
            1.0 - (1.0 - fade) * progress
        } else {
            // Sustain phase
            1.0
        }
    }
}

impl Default for FfEnvelope {
    fn default() -> Self {
        Self::new()
    }
}

/// Rumble effect parameters
#[derive(Debug, Clone, Copy)]
pub struct RumbleEffect {
    /// Strong/low frequency motor intensity (0-65535)
    pub strong_magnitude: u16,
    /// Weak/high frequency motor intensity (0-65535)
    pub weak_magnitude: u16,
}

impl RumbleEffect {
    pub const fn new(strong: u16, weak: u16) -> Self {
        Self {
            strong_magnitude: strong,
            weak_magnitude: weak,
        }
    }

    /// Full intensity rumble
    pub const fn full() -> Self {
        Self::new(65535, 65535)
    }

    /// Subtle rumble
    pub const fn subtle() -> Self {
        Self::new(16384, 8192)
    }

    /// Strong only
    pub const fn strong(intensity: u16) -> Self {
        Self::new(intensity, 0)
    }

    /// Weak only
    pub const fn weak(intensity: u16) -> Self {
        Self::new(0, intensity)
    }
}

impl Default for RumbleEffect {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// Periodic effect parameters
#[derive(Debug, Clone, Copy)]
pub struct PeriodicEffect {
    /// Waveform type
    pub waveform: FfWaveform,
    /// Period in milliseconds
    pub period: u16,
    /// Magnitude (0-32767)
    pub magnitude: i16,
    /// Offset (-32767 to 32767)
    pub offset: i16,
    /// Phase shift (0-65535 = 0-360 degrees)
    pub phase: u16,
    /// Envelope
    pub envelope: FfEnvelope,
}

impl PeriodicEffect {
    pub const fn new(waveform: FfWaveform, period: u16, magnitude: i16) -> Self {
        Self {
            waveform,
            period,
            magnitude,
            offset: 0,
            phase: 0,
            envelope: FfEnvelope::new(),
        }
    }

    /// Generate effect value at time t (milliseconds)
    pub fn sample(&self, t: u32) -> i16 {
        let phase_offset = self.phase as f32 / 65535.0;
        let t_normalized = (t % self.period as u32) as f32 / self.period as f32;
        let phase = (t_normalized + phase_offset) % 1.0;

        let wave = self.waveform.sample(phase);
        let value = self.offset as f32 + wave * self.magnitude as f32;
        value.clamp(-32767.0, 32767.0) as i16
    }
}

impl Default for PeriodicEffect {
    fn default() -> Self {
        Self::new(FfWaveform::Sine, 100, 16384)
    }
}

/// Constant force effect parameters
#[derive(Debug, Clone, Copy)]
pub struct ConstantEffect {
    /// Force level (-32767 to 32767)
    pub level: i16,
    /// Envelope
    pub envelope: FfEnvelope,
}

impl ConstantEffect {
    pub const fn new(level: i16) -> Self {
        Self {
            level,
            envelope: FfEnvelope::new(),
        }
    }
}

impl Default for ConstantEffect {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Ramp effect parameters
#[derive(Debug, Clone, Copy)]
pub struct RampEffect {
    /// Start level (-32767 to 32767)
    pub start_level: i16,
    /// End level (-32767 to 32767)
    pub end_level: i16,
    /// Envelope
    pub envelope: FfEnvelope,
}

impl RampEffect {
    pub const fn new(start: i16, end: i16) -> Self {
        Self {
            start_level: start,
            end_level: end,
            envelope: FfEnvelope::new(),
        }
    }

    /// Calculate level at time t (0.0 to 1.0 progress)
    pub fn level_at(&self, progress: f32) -> i16 {
        let start = self.start_level as f32;
        let end = self.end_level as f32;
        (start + (end - start) * progress) as i16
    }
}

impl Default for RampEffect {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// Condition effect parameters (for spring, damper, friction, inertia)
#[derive(Debug, Clone, Copy)]
pub struct ConditionEffect {
    /// Right saturation (0-65535)
    pub right_saturation: u16,
    /// Left saturation (0-65535)
    pub left_saturation: u16,
    /// Right coefficient (-32767 to 32767)
    pub right_coeff: i16,
    /// Left coefficient (-32767 to 32767)
    pub left_coeff: i16,
    /// Deadband (0-65535)
    pub deadband: u16,
    /// Center (-32767 to 32767)
    pub center: i16,
}

impl ConditionEffect {
    pub const fn new() -> Self {
        Self {
            right_saturation: 65535,
            left_saturation: 65535,
            right_coeff: 0,
            left_coeff: 0,
            deadband: 0,
            center: 0,
        }
    }

    /// Create a symmetric spring effect
    pub const fn spring(coefficient: i16, saturation: u16) -> Self {
        Self {
            right_saturation: saturation,
            left_saturation: saturation,
            right_coeff: coefficient,
            left_coeff: coefficient,
            deadband: 0,
            center: 0,
        }
    }

    /// Create a symmetric damper effect
    pub const fn damper(coefficient: i16, saturation: u16) -> Self {
        Self::spring(coefficient, saturation)
    }

    /// Calculate force for given position and velocity
    pub fn calculate(&self, position: i16, _velocity: i16) -> i16 {
        let relative_pos = position - self.center;

        // Apply deadband
        let dead = self.deadband as i16;
        if relative_pos.abs() < dead {
            return 0;
        }

        let pos_after_dead = if relative_pos > 0 {
            relative_pos - dead
        } else {
            relative_pos + dead
        };

        // Calculate force
        let (coeff, sat) = if pos_after_dead > 0 {
            (self.right_coeff, self.right_saturation)
        } else {
            (self.left_coeff, self.left_saturation)
        };

        let force = (pos_after_dead as i32 * coeff as i32) / 32767;
        force.clamp(-(sat as i32), sat as i32) as i16
    }
}

impl Default for ConditionEffect {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete force feedback effect
#[derive(Debug, Clone)]
pub struct FfEffect {
    /// Effect ID (-1 for new effect)
    pub id: i16,
    /// Effect type
    pub effect_type: FfEffectType,
    /// Direction
    pub direction: FfDirection,
    /// Replay parameters
    pub replay: FfReplay,
    /// Trigger parameters
    pub trigger: FfTrigger,
    /// Effect-specific data
    pub data: FfEffectData,
}

/// Effect-specific data
#[derive(Debug, Clone)]
pub enum FfEffectData {
    Rumble(RumbleEffect),
    Periodic(PeriodicEffect),
    Constant(ConstantEffect),
    Ramp(RampEffect),
    Condition([ConditionEffect; 2]), // X and Y axes
}

impl FfEffect {
    /// Create a new rumble effect
    pub fn rumble(strong: u16, weak: u16, duration_ms: u16) -> Self {
        Self {
            id: -1,
            effect_type: FfEffectType::Rumble,
            direction: FfDirection::default(),
            replay: FfReplay::new(duration_ms, 0),
            trigger: FfTrigger::none(),
            data: FfEffectData::Rumble(RumbleEffect::new(strong, weak)),
        }
    }

    /// Create a periodic effect
    pub fn periodic(waveform: FfWaveform, period_ms: u16, magnitude: i16, duration_ms: u16) -> Self {
        Self {
            id: -1,
            effect_type: FfEffectType::Periodic,
            direction: FfDirection::default(),
            replay: FfReplay::new(duration_ms, 0),
            trigger: FfTrigger::none(),
            data: FfEffectData::Periodic(PeriodicEffect::new(waveform, period_ms, magnitude)),
        }
    }

    /// Create a constant force effect
    pub fn constant(level: i16, duration_ms: u16, direction: FfDirection) -> Self {
        Self {
            id: -1,
            effect_type: FfEffectType::Constant,
            direction,
            replay: FfReplay::new(duration_ms, 0),
            trigger: FfTrigger::none(),
            data: FfEffectData::Constant(ConstantEffect::new(level)),
        }
    }

    /// Create a spring effect
    pub fn spring(coefficient: i16, saturation: u16) -> Self {
        let cond = ConditionEffect::spring(coefficient, saturation);
        Self {
            id: -1,
            effect_type: FfEffectType::Spring,
            direction: FfDirection::default(),
            replay: FfReplay::infinite(),
            trigger: FfTrigger::none(),
            data: FfEffectData::Condition([cond, cond]),
        }
    }

    /// Create a damper effect
    pub fn damper(coefficient: i16, saturation: u16) -> Self {
        let cond = ConditionEffect::damper(coefficient, saturation);
        Self {
            id: -1,
            effect_type: FfEffectType::Damper,
            direction: FfDirection::default(),
            replay: FfReplay::infinite(),
            trigger: FfTrigger::none(),
            data: FfEffectData::Condition([cond, cond]),
        }
    }
}

/// Effect playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfPlaybackState {
    /// Effect is stopped
    Stopped,
    /// Effect is playing
    Playing,
    /// Effect is paused
    Paused,
}

/// Effect playback instance
struct EffectInstance {
    /// Effect definition
    effect: FfEffect,
    /// Playback state
    state: FfPlaybackState,
    /// Number of iterations remaining (0 = infinite)
    iterations: u32,
    /// Start time (milliseconds)
    start_time: u32,
    /// Current time within effect (milliseconds)
    current_time: u32,
}

impl EffectInstance {
    fn new(effect: FfEffect) -> Self {
        Self {
            effect,
            state: FfPlaybackState::Stopped,
            iterations: 0,
            start_time: 0,
            current_time: 0,
        }
    }
}

/// Force feedback device
pub struct FfDevice {
    /// Device name
    name: String,
    /// Effects storage
    effects: RwLock<BTreeMap<i16, EffectInstance>>,
    /// Next effect ID
    next_id: AtomicI16,
    /// Maximum number of simultaneous effects
    max_effects: usize,
    /// Currently playing effects
    playing_count: AtomicU16,
    /// Global gain (0-65535)
    gain: AtomicU16,
    /// Autocenter strength (0-65535)
    autocenter: AtomicU16,
    /// Device output handler
    output_handler: RwLock<Option<Box<dyn FfOutputHandler>>>,
}

/// Trait for handling force feedback output
pub trait FfOutputHandler: Send + Sync {
    /// Send rumble command
    fn rumble(&self, strong: u16, weak: u16);

    /// Send constant force command
    fn constant_force(&self, x: i16, y: i16);

    /// Set gain
    fn set_gain(&self, gain: u16);

    /// Set autocenter
    fn set_autocenter(&self, strength: u16);

    /// Stop all effects
    fn stop_all(&self);
}

impl FfDevice {
    pub fn new(name: &str, max_effects: usize) -> Self {
        Self {
            name: String::from(name),
            effects: RwLock::new(BTreeMap::new()),
            next_id: AtomicI16::new(0),
            max_effects,
            playing_count: AtomicU16::new(0),
            gain: AtomicU16::new(65535),
            autocenter: AtomicU16::new(0),
            output_handler: RwLock::new(None),
        }
    }

    /// Set output handler
    pub fn set_output_handler<H: FfOutputHandler + 'static>(&self, handler: H) {
        *self.output_handler.write() = Some(Box::new(handler));
    }

    /// Upload an effect
    pub fn upload_effect(&self, mut effect: FfEffect) -> Result<i16, InputError> {
        if effect.id < 0 {
            // New effect
            let effects = self.effects.read();
            if effects.len() >= self.max_effects {
                return Err(InputError::NoMemory);
            }
            drop(effects);

            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            effect.id = id;

            let instance = EffectInstance::new(effect);
            self.effects.write().insert(id, instance);
            Ok(id)
        } else {
            // Update existing effect
            let mut effects = self.effects.write();
            if let Some(instance) = effects.get_mut(&effect.id) {
                instance.effect = effect.clone();
                Ok(effect.id)
            } else {
                Err(InputError::InvalidArgument)
            }
        }
    }

    /// Erase an effect
    pub fn erase_effect(&self, id: i16) -> Result<(), InputError> {
        let mut effects = self.effects.write();
        if effects.remove(&id).is_some() {
            Ok(())
        } else {
            Err(InputError::InvalidArgument)
        }
    }

    /// Play an effect
    pub fn play_effect(&self, id: i16, count: u32) -> Result<(), InputError> {
        let mut effects = self.effects.write();
        if let Some(instance) = effects.get_mut(&id) {
            instance.state = FfPlaybackState::Playing;
            instance.iterations = count;
            instance.start_time = 0; // Will be set by update loop
            instance.current_time = 0;
            self.playing_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        } else {
            Err(InputError::InvalidArgument)
        }
    }

    /// Stop an effect
    pub fn stop_effect(&self, id: i16) -> Result<(), InputError> {
        let mut effects = self.effects.write();
        if let Some(instance) = effects.get_mut(&id) {
            if instance.state == FfPlaybackState::Playing {
                instance.state = FfPlaybackState::Stopped;
                self.playing_count.fetch_sub(1, Ordering::SeqCst);
            }
            Ok(())
        } else {
            Err(InputError::InvalidArgument)
        }
    }

    /// Stop all effects
    pub fn stop_all(&self) {
        let mut effects = self.effects.write();
        for instance in effects.values_mut() {
            instance.state = FfPlaybackState::Stopped;
        }
        self.playing_count.store(0, Ordering::SeqCst);

        if let Some(ref handler) = *self.output_handler.read() {
            handler.stop_all();
        }
    }

    /// Set global gain
    pub fn set_gain(&self, gain: u16) {
        self.gain.store(gain, Ordering::SeqCst);
        if let Some(ref handler) = *self.output_handler.read() {
            handler.set_gain(gain);
        }
    }

    /// Get global gain
    pub fn gain(&self) -> u16 {
        self.gain.load(Ordering::SeqCst)
    }

    /// Set autocenter strength
    pub fn set_autocenter(&self, strength: u16) {
        self.autocenter.store(strength, Ordering::SeqCst);
        if let Some(ref handler) = *self.output_handler.read() {
            handler.set_autocenter(strength);
        }
    }

    /// Get autocenter strength
    pub fn autocenter(&self) -> u16 {
        self.autocenter.load(Ordering::SeqCst)
    }

    /// Update effects (call periodically)
    pub fn update(&self, current_time_ms: u32) {
        let mut total_rumble_strong: u32 = 0;
        let mut total_rumble_weak: u32 = 0;
        let mut total_force_x: i32 = 0;
        let mut total_force_y: i32 = 0;

        let gain = self.gain.load(Ordering::SeqCst) as u32;

        let mut effects = self.effects.write();
        let mut completed: Vec<i16> = Vec::new();

        for (id, instance) in effects.iter_mut() {
            if instance.state != FfPlaybackState::Playing {
                continue;
            }

            // Initialize start time if needed
            if instance.start_time == 0 {
                instance.start_time = current_time_ms;
            }

            // Handle delay
            let effect_time = current_time_ms.saturating_sub(instance.start_time);
            if effect_time < instance.effect.replay.delay as u32 {
                continue;
            }

            let time_in_effect = effect_time - instance.effect.replay.delay as u32;
            instance.current_time = time_in_effect as u32;

            // Check if effect has completed
            let duration = instance.effect.replay.duration as u32;
            if duration > 0 && time_in_effect >= duration {
                if instance.iterations > 1 {
                    instance.iterations -= 1;
                    instance.start_time = current_time_ms;
                } else if instance.iterations == 1 {
                    completed.push(*id);
                    continue;
                }
                // iterations == 0 means infinite
            }

            // Calculate effect output
            match &instance.effect.data {
                FfEffectData::Rumble(rumble) => {
                    let envelope = 1.0; // No envelope for rumble
                    total_rumble_strong += (rumble.strong_magnitude as f32 * envelope) as u32;
                    total_rumble_weak += (rumble.weak_magnitude as f32 * envelope) as u32;
                }
                FfEffectData::Periodic(periodic) => {
                    let value = periodic.sample(time_in_effect);
                    let envelope = periodic.envelope.multiplier(
                        time_in_effect as u16,
                        duration as u16,
                    );
                    let force = (value as f32 * envelope) as i32;
                    let (dx, dy) = instance.effect.direction.components();
                    total_force_x += (force as f32 * dx) as i32;
                    total_force_y += (force as f32 * dy) as i32;
                }
                FfEffectData::Constant(constant) => {
                    let envelope = constant.envelope.multiplier(
                        time_in_effect as u16,
                        duration as u16,
                    );
                    let force = (constant.level as f32 * envelope) as i32;
                    let (dx, dy) = instance.effect.direction.components();
                    total_force_x += (force as f32 * dx) as i32;
                    total_force_y += (force as f32 * dy) as i32;
                }
                FfEffectData::Ramp(ramp) => {
                    let progress = if duration > 0 {
                        time_in_effect as f32 / duration as f32
                    } else {
                        0.0
                    };
                    let level = ramp.level_at(progress);
                    let envelope = ramp.envelope.multiplier(
                        time_in_effect as u16,
                        duration as u16,
                    );
                    let force = (level as f32 * envelope) as i32;
                    let (dx, dy) = instance.effect.direction.components();
                    total_force_x += (force as f32 * dx) as i32;
                    total_force_y += (force as f32 * dy) as i32;
                }
                FfEffectData::Condition(_conditions) => {
                    // Condition effects need position input
                    // This would be handled by the device driver
                }
            }
        }

        // Mark completed effects
        for id in completed {
            if let Some(instance) = effects.get_mut(&id) {
                instance.state = FfPlaybackState::Stopped;
                self.playing_count.fetch_sub(1, Ordering::SeqCst);
            }
        }

        drop(effects);

        // Apply gain and send output
        if let Some(ref handler) = *self.output_handler.read() {
            // Clamp and apply gain to rumble
            let strong = ((total_rumble_strong * gain / 65535) as u32).min(65535) as u16;
            let weak = ((total_rumble_weak * gain / 65535) as u32).min(65535) as u16;
            if strong > 0 || weak > 0 {
                handler.rumble(strong, weak);
            }

            // Clamp and apply gain to force
            let fx = ((total_force_x * gain as i32 / 65535) as i32).clamp(-32767, 32767) as i16;
            let fy = ((total_force_y * gain as i32 / 65535) as i32).clamp(-32767, 32767) as i16;
            if fx != 0 || fy != 0 {
                handler.constant_force(fx, fy);
            }
        }
    }

    /// Get number of playing effects
    pub fn playing_count(&self) -> u16 {
        self.playing_count.load(Ordering::SeqCst)
    }

    /// Get effect count
    pub fn effect_count(&self) -> usize {
        self.effects.read().len()
    }

    /// Get max effects
    pub fn max_effects(&self) -> usize {
        self.max_effects
    }
}

/// Simple rumble output handler for gamepads
pub struct GamepadRumbleHandler {
    /// Callback for rumble output
    callback: Box<dyn Fn(u16, u16) + Send + Sync>,
}

impl GamepadRumbleHandler {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(u16, u16) + Send + Sync + 'static,
    {
        Self {
            callback: Box::new(callback),
        }
    }
}

impl FfOutputHandler for GamepadRumbleHandler {
    fn rumble(&self, strong: u16, weak: u16) {
        (self.callback)(strong, weak);
    }

    fn constant_force(&self, _x: i16, _y: i16) {
        // Not supported on simple rumble devices
    }

    fn set_gain(&self, _gain: u16) {}

    fn set_autocenter(&self, _strength: u16) {}

    fn stop_all(&self) {
        (self.callback)(0, 0);
    }
}

/// Initialize force feedback subsystem
pub fn init() {
    // FF devices are initialized when hardware is detected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waveform_sine() {
        let wave = FfWaveform::Sine;
        assert!((wave.sample(0.0) - 0.0).abs() < 0.01);
        assert!((wave.sample(0.25) - 1.0).abs() < 0.01);
        assert!((wave.sample(0.5) - 0.0).abs() < 0.01);
        assert!((wave.sample(0.75) - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_waveform_square() {
        let wave = FfWaveform::Square;
        assert_eq!(wave.sample(0.25), 1.0);
        assert_eq!(wave.sample(0.75), -1.0);
    }

    #[test]
    fn test_envelope() {
        let env = FfEnvelope {
            attack_length: 100,
            attack_level: 0,
            fade_length: 100,
            fade_level: 0,
        };

        // Attack phase
        assert!(env.multiplier(0, 1000) < 0.01);
        assert!((env.multiplier(50, 1000) - 0.5).abs() < 0.01);
        assert!((env.multiplier(100, 1000) - 1.0).abs() < 0.01);

        // Sustain phase
        assert!((env.multiplier(500, 1000) - 1.0).abs() < 0.01);

        // Fade phase
        assert!((env.multiplier(950, 1000) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_direction() {
        let down = FfDirection::down();
        let (x, y) = down.components();
        assert!(x.abs() < 0.01);
        assert!((y - 1.0).abs() < 0.01);

        let right = FfDirection::right();
        let (x, y) = right.components();
        assert!((x - 1.0).abs() < 0.01);
        assert!(y.abs() < 0.01);
    }

    #[test]
    fn test_ff_device() {
        let device = FfDevice::new("Test Device", 8);

        // Upload effect
        let effect = FfEffect::rumble(32768, 16384, 1000);
        let id = device.upload_effect(effect).unwrap();
        assert_eq!(id, 0);

        // Play effect
        device.play_effect(id, 1).unwrap();
        assert_eq!(device.playing_count(), 1);

        // Stop effect
        device.stop_effect(id).unwrap();
        assert_eq!(device.playing_count(), 0);

        // Erase effect
        device.erase_effect(id).unwrap();
        assert_eq!(device.effect_count(), 0);
    }

    #[test]
    fn test_periodic_effect() {
        let periodic = PeriodicEffect::new(FfWaveform::Sine, 100, 32767);
        let sample = periodic.sample(25); // 1/4 period
        assert!(sample > 32000); // Should be near max
    }
}
