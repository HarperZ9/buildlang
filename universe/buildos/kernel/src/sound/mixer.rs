//! Audio Mixer
//!
//! ALSA-style mixer controls:
//! - Volume controls
//! - Switch controls
//! - Enumerated controls
//! - dB scale support

use alloc::collections::BTreeMap;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use spin::RwLock;
use super::SoundError;

/// Control type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlType {
    /// Boolean switch
    Boolean,
    /// Integer
    Integer,
    /// Integer64
    Integer64,
    /// Enumerated
    Enumerated,
    /// Bytes
    Bytes,
    /// IEC958 (S/PDIF)
    Iec958,
    /// Volume control
    Volume,
}

/// Element type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementType {
    /// None
    None,
    /// Playback switch
    PlaybackSwitch,
    /// Playback volume
    PlaybackVolume,
    /// Capture switch
    CaptureSwitch,
    /// Capture volume
    CaptureVolume,
    /// Common switch
    CommonSwitch,
    /// Common volume
    CommonVolume,
    /// Enumerated
    Enumerated,
}

/// Mixer control access flags
#[derive(Clone, Copy, Debug, Default)]
pub struct ControlAccess {
    /// Readable
    pub read: bool,
    /// Writable
    pub write: bool,
    /// Volatile (can change without notification)
    pub volatile: bool,
    /// TLV (Type-Length-Value) readable
    pub tlv_read: bool,
    /// TLV writable
    pub tlv_write: bool,
    /// TLV commandable
    pub tlv_command: bool,
    /// Inactive (disabled)
    pub inactive: bool,
    /// Lock (changes temporarily disabled)
    pub lock: bool,
    /// User space accessible
    pub user: bool,
}

impl ControlAccess {
    /// Default readable/writable control
    pub fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            ..Default::default()
        }
    }

    /// Read-only control
    pub fn read_only() -> Self {
        Self {
            read: true,
            ..Default::default()
        }
    }
}

/// Mixer control information
#[derive(Clone, Debug)]
pub struct ControlInfo {
    /// Control ID
    pub id: u32,
    /// Control type
    pub ctrl_type: ControlType,
    /// Access flags
    pub access: ControlAccess,
    /// Number of values (channels)
    pub count: u32,
}

/// Integer control info
#[derive(Clone, Debug)]
pub struct IntegerInfo {
    /// Minimum value
    pub min: i64,
    /// Maximum value
    pub max: i64,
    /// Step
    pub step: i64,
}

/// Enumerated control info
#[derive(Clone, Debug)]
pub struct EnumeratedInfo {
    /// Number of items
    pub items: u32,
    /// Item names
    pub names: Vec<String>,
}

/// dB scale info
#[derive(Clone, Copy, Debug)]
pub struct DbScale {
    /// Minimum dB value (in 0.01 dB units)
    pub min: i32,
    /// Step (in 0.01 dB units)
    pub step: i32,
    /// Is mute at minimum
    pub mute: bool,
}

impl DbScale {
    /// Convert raw value to dB
    pub fn to_db(&self, raw: i32, info: &IntegerInfo) -> i32 {
        let range = info.max - info.min;
        if range == 0 {
            return self.min;
        }
        let normalized = (raw as i64 - info.min) * 100 / range;
        self.min + (normalized as i32 * (self.step / 100))
    }

    /// Convert dB to raw value
    pub fn from_db(&self, db: i32, info: &IntegerInfo) -> i32 {
        if db <= self.min && self.mute {
            return info.min as i32;
        }
        let range = info.max - info.min;
        let steps = (db - self.min) / (self.step / 100);
        let raw = info.min as i32 + ((steps as i64 * range) / 100) as i32;
        raw.max(info.min as i32).min(info.max as i32)
    }
}

/// Mixer control value
#[derive(Clone, Debug)]
pub enum ControlValue {
    /// Boolean values
    Boolean(Vec<bool>),
    /// Integer values
    Integer(Vec<i64>),
    /// Enumerated value
    Enumerated(Vec<u32>),
    /// Byte data
    Bytes(Vec<u8>),
}

/// Mixer control
pub struct MixerControl {
    /// Control name
    pub name: String,
    /// Control ID
    pub id: u32,
    /// Control type
    pub ctrl_type: ControlType,
    /// Element type
    pub elem_type: ElementType,
    /// Access flags
    pub access: ControlAccess,
    /// Number of channels
    pub count: u32,
    /// Integer info (if applicable)
    pub int_info: Option<IntegerInfo>,
    /// Enumerated info (if applicable)
    pub enum_info: Option<EnumeratedInfo>,
    /// dB scale (if applicable)
    pub db_scale: Option<DbScale>,
    /// Current value
    value: RwLock<ControlValue>,
    /// Is muted
    muted: AtomicBool,
    /// Hardware write callback
    write_callback: Option<Box<dyn Fn(&ControlValue) -> Result<(), SoundError> + Send + Sync>>,
}

impl MixerControl {
    /// Create a generic mixer control
    pub fn new(name: &str, id: u32, ctrl_type: ControlType, count: u32) -> Self {
        Self {
            name: String::from(name),
            id,
            ctrl_type,
            elem_type: ElementType::CommonVolume,
            access: ControlAccess::read_write(),
            count,
            int_info: Some(IntegerInfo { min: 0, max: 100, step: 1 }),
            enum_info: None,
            db_scale: None,
            value: RwLock::new(ControlValue::Integer(vec![0; count as usize])),
            muted: AtomicBool::new(false),
            write_callback: None,
        }
    }

    /// Create boolean control
    pub fn new_boolean(name: &str, id: u32, count: u32) -> Self {
        Self {
            name: String::from(name),
            id,
            ctrl_type: ControlType::Boolean,
            elem_type: ElementType::CommonSwitch,
            access: ControlAccess::read_write(),
            count,
            int_info: None,
            enum_info: None,
            db_scale: None,
            value: RwLock::new(ControlValue::Boolean(vec![false; count as usize])),
            muted: AtomicBool::new(false),
            write_callback: None,
        }
    }

    /// Create integer control
    pub fn new_integer(name: &str, id: u32, count: u32, min: i64, max: i64, step: i64) -> Self {
        Self {
            name: String::from(name),
            id,
            ctrl_type: ControlType::Integer,
            elem_type: ElementType::CommonVolume,
            access: ControlAccess::read_write(),
            count,
            int_info: Some(IntegerInfo { min, max, step }),
            enum_info: None,
            db_scale: None,
            value: RwLock::new(ControlValue::Integer(vec![max; count as usize])),
            muted: AtomicBool::new(false),
            write_callback: None,
        }
    }

    /// Create enumerated control
    pub fn new_enumerated(name: &str, id: u32, items: Vec<String>) -> Self {
        let items_count = items.len() as u32;
        Self {
            name: String::from(name),
            id,
            ctrl_type: ControlType::Enumerated,
            elem_type: ElementType::Enumerated,
            access: ControlAccess::read_write(),
            count: 1,
            int_info: None,
            enum_info: Some(EnumeratedInfo {
                items: items_count,
                names: items,
            }),
            db_scale: None,
            value: RwLock::new(ControlValue::Enumerated(vec![0])),
            muted: AtomicBool::new(false),
            write_callback: None,
        }
    }

    /// Set dB scale
    pub fn set_db_scale(&mut self, scale: DbScale) {
        self.db_scale = Some(scale);
    }

    /// Set element type
    pub fn set_element_type(&mut self, elem_type: ElementType) {
        self.elem_type = elem_type;
    }

    /// Set range for integer controls
    pub fn set_range(&mut self, min: i64, max: i64) {
        self.int_info = Some(IntegerInfo { min, max, step: 1 });
    }

    /// Set write callback
    pub fn set_write_callback<F>(&mut self, callback: F)
    where
        F: Fn(&ControlValue) -> Result<(), SoundError> + Send + Sync + 'static,
    {
        self.write_callback = Some(Box::new(callback));
    }

    /// Get value
    pub fn get_value(&self) -> ControlValue {
        self.value.read().clone()
    }

    /// Set value
    pub fn set_value(&self, value: ControlValue) -> Result<(), SoundError> {
        if !self.access.write {
            return Err(SoundError::NotSupported);
        }

        // Validate value
        self.validate_value(&value)?;

        // Call hardware callback
        if let Some(ref callback) = self.write_callback {
            callback(&value)?;
        }

        *self.value.write() = value;
        Ok(())
    }

    /// Validate value
    fn validate_value(&self, value: &ControlValue) -> Result<(), SoundError> {
        match (self.ctrl_type, value) {
            (ControlType::Boolean, ControlValue::Boolean(vals)) => {
                if vals.len() != self.count as usize {
                    return Err(SoundError::InvalidParameter);
                }
            }
            (ControlType::Integer, ControlValue::Integer(vals)) => {
                if vals.len() != self.count as usize {
                    return Err(SoundError::InvalidParameter);
                }
                if let Some(ref info) = self.int_info {
                    for v in vals {
                        if *v < info.min || *v > info.max {
                            return Err(SoundError::InvalidParameter);
                        }
                    }
                }
            }
            (ControlType::Enumerated, ControlValue::Enumerated(vals)) => {
                if let Some(ref info) = self.enum_info {
                    for v in vals {
                        if *v >= info.items {
                            return Err(SoundError::InvalidParameter);
                        }
                    }
                }
            }
            _ => return Err(SoundError::InvalidParameter),
        }
        Ok(())
    }

    /// Get volume in dB
    pub fn get_volume_db(&self, channel: usize) -> Option<i32> {
        let db_scale = self.db_scale.as_ref()?;
        let int_info = self.int_info.as_ref()?;
        if let ControlValue::Integer(vals) = &*self.value.read() {
            if channel < vals.len() {
                return Some(db_scale.to_db(vals[channel] as i32, int_info));
            }
        }
        None
    }

    /// Set volume in dB
    pub fn set_volume_db(&self, channel: usize, db: i32) -> Result<(), SoundError> {
        let db_scale = self.db_scale.as_ref().ok_or(SoundError::NotSupported)?;
        let int_info = self.int_info.as_ref().ok_or(SoundError::NotSupported)?;
        let raw = db_scale.from_db(db, int_info);
        let mut value = self.value.write();
        if let ControlValue::Integer(ref mut vals) = *value {
            if channel < vals.len() {
                vals[channel] = raw as i64;
                return Ok(());
            }
        }
        Err(SoundError::InvalidParameter)
    }

    /// Get percent volume (0-100)
    pub fn get_volume_percent(&self, channel: usize) -> Option<u8> {
        let int_info = self.int_info.as_ref()?;
        if let ControlValue::Integer(vals) = &*self.value.read() {
            if channel < vals.len() {
                let range = int_info.max - int_info.min;
                if range > 0 {
                    let pct = ((vals[channel] - int_info.min) * 100) / range;
                    return Some(pct as u8);
                }
            }
        }
        None
    }

    /// Set percent volume (0-100)
    pub fn set_volume_percent(&self, channel: usize, percent: u8) -> Result<(), SoundError> {
        let int_info = self.int_info.as_ref().ok_or(SoundError::NotSupported)?;
        let range = int_info.max - int_info.min;
        let raw = int_info.min + (percent as i64 * range) / 100;
        let mut value = self.value.write();
        if let ControlValue::Integer(ref mut vals) = *value {
            if channel < vals.len() {
                vals[channel] = raw;
                return Ok(());
            }
        }
        Err(SoundError::InvalidParameter)
    }

    /// Is muted
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Set muted
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
    }
}

impl Clone for MixerControl {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            id: self.id,
            ctrl_type: self.ctrl_type,
            elem_type: self.elem_type,
            access: self.access,
            count: self.count,
            int_info: self.int_info.clone(),
            enum_info: self.enum_info.clone(),
            db_scale: self.db_scale,
            value: RwLock::new(self.value.read().clone()),
            muted: AtomicBool::new(self.muted.load(Ordering::Relaxed)),
            write_callback: None, // Callbacks cannot be cloned
        }
    }
}

/// Mixer element (group of related controls)
pub struct MixerElement {
    /// Element name
    pub name: String,
    /// Element index
    pub index: u32,
    /// Playback volume control
    pub playback_volume: Option<Arc<MixerControl>>,
    /// Playback switch control
    pub playback_switch: Option<Arc<MixerControl>>,
    /// Capture volume control
    pub capture_volume: Option<Arc<MixerControl>>,
    /// Capture switch control
    pub capture_switch: Option<Arc<MixerControl>>,
    /// Enumerated control
    pub enumerated: Option<Arc<MixerControl>>,
}

impl MixerElement {
    /// Create new mixer element
    pub fn new(name: &str, index: u32) -> Self {
        Self {
            name: String::from(name),
            index,
            playback_volume: None,
            playback_switch: None,
            capture_volume: None,
            capture_switch: None,
            enumerated: None,
        }
    }

    /// Add a control to this element
    pub fn add_control(&mut self, control: MixerControl) {
        let control = Arc::new(control);
        match control.ctrl_type {
            ControlType::Volume | ControlType::Integer => {
                self.playback_volume = Some(control);
            }
            ControlType::Boolean => {
                self.playback_switch = Some(control);
            }
            ControlType::Enumerated => {
                self.enumerated = Some(control);
            }
            _ => {}
        }
    }

    /// Has playback volume
    pub fn has_playback_volume(&self) -> bool {
        self.playback_volume.is_some()
    }

    /// Has playback switch
    pub fn has_playback_switch(&self) -> bool {
        self.playback_switch.is_some()
    }

    /// Has capture volume
    pub fn has_capture_volume(&self) -> bool {
        self.capture_volume.is_some()
    }

    /// Has capture switch
    pub fn has_capture_switch(&self) -> bool {
        self.capture_switch.is_some()
    }

    /// Get playback volume percent
    pub fn get_playback_volume(&self, channel: usize) -> Option<u8> {
        self.playback_volume.as_ref()?.get_volume_percent(channel)
    }

    /// Set playback volume percent
    pub fn set_playback_volume(&self, channel: usize, percent: u8) -> Result<(), SoundError> {
        self.playback_volume.as_ref()
            .ok_or(SoundError::NotSupported)?
            .set_volume_percent(channel, percent)
    }

    /// Get capture volume percent
    pub fn get_capture_volume(&self, channel: usize) -> Option<u8> {
        self.capture_volume.as_ref()?.get_volume_percent(channel)
    }

    /// Set capture volume percent
    pub fn set_capture_volume(&self, channel: usize, percent: u8) -> Result<(), SoundError> {
        self.capture_volume.as_ref()
            .ok_or(SoundError::NotSupported)?
            .set_volume_percent(channel, percent)
    }

    /// Is playback muted
    pub fn is_playback_muted(&self) -> bool {
        self.playback_switch.as_ref()
            .map(|c| {
                if let ControlValue::Boolean(vals) = c.get_value() {
                    vals.iter().all(|&v| !v)
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// Set playback mute
    pub fn set_playback_mute(&self, muted: bool) -> Result<(), SoundError> {
        let ctrl = self.playback_switch.as_ref().ok_or(SoundError::NotSupported)?;
        let count = ctrl.count as usize;
        ctrl.set_value(ControlValue::Boolean(vec![!muted; count]))
    }
}

/// Mixer
pub struct Mixer {
    /// Mixer name
    pub name: String,
    /// Card ID
    pub card_id: u32,
    /// Controls
    controls: RwLock<BTreeMap<u32, Arc<MixerControl>>>,
    /// Elements
    elements: RwLock<BTreeMap<String, Arc<MixerElement>>>,
    /// Next control ID
    next_control_id: AtomicI32,
}

impl Mixer {
    /// Create new mixer
    pub fn new(name: &str, card_id: u32) -> Self {
        Self {
            name: String::from(name),
            card_id,
            controls: RwLock::new(BTreeMap::new()),
            elements: RwLock::new(BTreeMap::new()),
            next_control_id: AtomicI32::new(0),
        }
    }

    /// Add control
    pub fn add_control(&self, control: MixerControl) -> u32 {
        let id = control.id;
        self.controls.write().insert(id, Arc::new(control));
        id
    }

    /// Remove control
    pub fn remove_control(&self, id: u32) {
        self.controls.write().remove(&id);
    }

    /// Get control
    pub fn get_control(&self, id: u32) -> Option<Arc<MixerControl>> {
        self.controls.read().get(&id).cloned()
    }

    /// Get control by name
    pub fn get_control_by_name(&self, name: &str) -> Option<Arc<MixerControl>> {
        self.controls.read()
            .values()
            .find(|c| c.name == name)
            .cloned()
    }

    /// Get all controls
    pub fn get_controls(&self) -> Vec<Arc<MixerControl>> {
        self.controls.read().values().cloned().collect()
    }

    /// Add element
    pub fn add_element(&self, element: MixerElement) {
        self.elements.write().insert(element.name.clone(), Arc::new(element));
    }

    /// Get element
    pub fn get_element(&self, name: &str) -> Option<Arc<MixerElement>> {
        self.elements.read().get(name).cloned()
    }

    /// Get all elements
    pub fn get_elements(&self) -> Vec<Arc<MixerElement>> {
        self.elements.read().values().cloned().collect()
    }

    /// Get master volume (0-100)
    pub fn get_master_volume(&self) -> Option<u8> {
        self.get_element("Master")
            .and_then(|e| e.get_playback_volume(0))
    }

    /// Set master volume (0-100)
    pub fn set_master_volume(&self, percent: u8) -> Result<(), SoundError> {
        let master = self.get_element("Master").ok_or(SoundError::NoDevice)?;
        // Set all channels
        if let Some(ref vol) = master.playback_volume {
            for ch in 0..vol.count as usize {
                master.set_playback_volume(ch, percent)?;
            }
        }
        Ok(())
    }

    /// Is master muted
    pub fn is_master_muted(&self) -> bool {
        self.get_element("Master")
            .map(|e| e.is_playback_muted())
            .unwrap_or(false)
    }

    /// Set master mute
    pub fn set_master_mute(&self, muted: bool) -> Result<(), SoundError> {
        self.get_element("Master")
            .ok_or(SoundError::NoDevice)?
            .set_playback_mute(muted)
    }

    /// Allocate control ID
    pub fn alloc_control_id(&self) -> u32 {
        self.next_control_id.fetch_add(1, Ordering::SeqCst) as u32
    }
}

/// Create standard mixer elements
pub fn create_standard_elements(mixer: &Mixer) {
    // Master element
    let mut master = MixerElement::new("Master", 0);
    let mut master_vol = MixerControl::new_integer(
        "Master Playback Volume",
        mixer.alloc_control_id(),
        2,
        0, 100, 1,
    );
    master_vol.set_element_type(ElementType::PlaybackVolume);
    master_vol.set_db_scale(DbScale { min: -6400, step: 100, mute: true });

    let mut master_switch = MixerControl::new_boolean(
        "Master Playback Switch",
        mixer.alloc_control_id(),
        2,
    );
    master_switch.set_element_type(ElementType::PlaybackSwitch);

    mixer.add_control(master_vol.clone());
    mixer.add_control(master_switch.clone());
    master.playback_volume = Some(Arc::new(master_vol));
    master.playback_switch = Some(Arc::new(master_switch));
    mixer.add_element(master);

    // PCM element
    let mut pcm = MixerElement::new("PCM", 0);
    let mut pcm_vol = MixerControl::new_integer(
        "PCM Playback Volume",
        mixer.alloc_control_id(),
        2,
        0, 100, 1,
    );
    pcm_vol.set_element_type(ElementType::PlaybackVolume);
    mixer.add_control(pcm_vol.clone());
    pcm.playback_volume = Some(Arc::new(pcm_vol));
    mixer.add_element(pcm);

    // Capture element
    let mut capture = MixerElement::new("Capture", 0);
    let mut capture_vol = MixerControl::new_integer(
        "Capture Volume",
        mixer.alloc_control_id(),
        2,
        0, 100, 1,
    );
    capture_vol.set_element_type(ElementType::CaptureVolume);

    let mut capture_switch = MixerControl::new_boolean(
        "Capture Switch",
        mixer.alloc_control_id(),
        2,
    );
    capture_switch.set_element_type(ElementType::CaptureSwitch);

    mixer.add_control(capture_vol.clone());
    mixer.add_control(capture_switch.clone());
    capture.capture_volume = Some(Arc::new(capture_vol));
    capture.capture_switch = Some(Arc::new(capture_switch));
    mixer.add_element(capture);
}
