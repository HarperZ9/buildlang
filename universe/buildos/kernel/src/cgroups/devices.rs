// ===============================================================================
// BUILDOS KERNEL - CGROUPS DEVICES CONTROLLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Devices Controller for cgroups v2
//!
//! Provides device access control for cgroups.
//! Uses BPF-based device filtering in cgroups v2.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::CgroupError;

/// Initialize devices controller
pub fn init() {
    crate::kprintln!("[CGROUPS] Devices controller initialized");
}

/// Devices controller state
#[derive(Clone)]
pub struct DevicesController {
    /// Access rules
    pub rules: Vec<DeviceRule>,
    /// Default policy (allow/deny all)
    pub default_allow: bool,
}

impl DevicesController {
    /// Create new devices controller
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            default_allow: true,
        }
    }

    /// Check if access is allowed
    pub fn check_access(
        &self,
        device_type: DeviceType,
        major: u32,
        minor: u32,
        access: Access,
    ) -> bool {
        // Check rules in order (first match wins)
        for rule in &self.rules {
            if rule.matches(device_type, major, minor, access) {
                return rule.allow;
            }
        }

        // Default policy
        self.default_allow
    }

    /// Add a rule
    pub fn add_rule(&mut self, rule: DeviceRule) {
        self.rules.push(rule);
    }

    /// Clear all rules
    pub fn clear_rules(&mut self) {
        self.rules.clear();
    }
}

impl Default for DevicesController {
    fn default() -> Self {
        Self::new()
    }
}

/// Device type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceType {
    /// Any device type
    All,
    /// Character device
    Char,
    /// Block device
    Block,
}

impl DeviceType {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'a' => Some(Self::All),
            'c' => Some(Self::Char),
            'b' => Some(Self::Block),
            _ => None,
        }
    }

    pub fn to_char(&self) -> char {
        match self {
            Self::All => 'a',
            Self::Char => 'c',
            Self::Block => 'b',
        }
    }
}

/// Access flags
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub read: bool,
    pub write: bool,
    pub mknod: bool,
}

impl Access {
    pub fn all() -> Self {
        Self {
            read: true,
            write: true,
            mknod: true,
        }
    }

    pub fn from_str(s: &str) -> Self {
        Self {
            read: s.contains('r'),
            write: s.contains('w'),
            mknod: s.contains('m'),
        }
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        if self.read {
            s.push('r');
        }
        if self.write {
            s.push('w');
        }
        if self.mknod {
            s.push('m');
        }
        s
    }

    pub fn matches(&self, other: &Access) -> bool {
        (self.read && other.read) ||
        (self.write && other.write) ||
        (self.mknod && other.mknod)
    }
}

/// Device access rule
#[derive(Clone, Debug)]
pub struct DeviceRule {
    /// Allow or deny
    pub allow: bool,
    /// Device type
    pub device_type: DeviceType,
    /// Major number (* = any)
    pub major: Option<u32>,
    /// Minor number (* = any)
    pub minor: Option<u32>,
    /// Access flags
    pub access: Access,
}

impl DeviceRule {
    /// Check if rule matches a device access
    pub fn matches(
        &self,
        device_type: DeviceType,
        major: u32,
        minor: u32,
        access: Access,
    ) -> bool {
        // Check device type
        if self.device_type != DeviceType::All && self.device_type != device_type {
            return false;
        }

        // Check major number
        if let Some(rule_major) = self.major {
            if rule_major != major {
                return false;
            }
        }

        // Check minor number
        if let Some(rule_minor) = self.minor {
            if rule_minor != minor {
                return false;
            }
        }

        // Check access
        self.access.matches(&access)
    }

    /// Format rule for display
    pub fn format(&self) -> String {
        let action = if self.allow { "allow" } else { "deny" };
        let major = self.major.map(|m| m.to_string()).unwrap_or("*".to_string());
        let minor = self.minor.map(|m| m.to_string()).unwrap_or("*".to_string());

        alloc::format!(
            "{} {} {}:{} {}",
            action,
            self.device_type.to_char(),
            major,
            minor,
            self.access.to_string()
        )
    }

    /// Parse rule from string
    pub fn parse(s: &str) -> Result<Self, CgroupError> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(CgroupError::InvalidPath);
        }

        let allow = match parts[0] {
            "allow" => true,
            "deny" => false,
            _ => return Err(CgroupError::InvalidPath),
        };

        let device_type = parts[1].chars().next()
            .and_then(DeviceType::from_char)
            .ok_or(CgroupError::InvalidPath)?;

        let dev_parts: Vec<&str> = parts[2].split(':').collect();
        if dev_parts.len() != 2 {
            return Err(CgroupError::InvalidPath);
        }

        let major = if dev_parts[0] == "*" {
            None
        } else {
            Some(dev_parts[0].parse().map_err(|_| CgroupError::InvalidPath)?)
        };

        let minor = if dev_parts[1] == "*" {
            None
        } else {
            Some(dev_parts[1].parse().map_err(|_| CgroupError::InvalidPath)?)
        };

        let access = Access::from_str(parts[3]);

        Ok(Self {
            allow,
            device_type,
            major,
            minor,
            access,
        })
    }
}

/// Read a devices controller file
pub fn read_file(controller: &DevicesController, file: &str) -> Result<String, CgroupError> {
    match file {
        "list" => {
            let mut output = String::new();
            for rule in &controller.rules {
                output.push_str(&rule.format());
                output.push('\n');
            }
            Ok(output.trim_end().to_string())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Write a devices controller file
pub fn write_file(controller: &mut DevicesController, file: &str, value: &str) -> Result<(), CgroupError> {
    match file {
        "allow" => {
            let rule = parse_simple_rule(value, true)?;
            controller.rules.push(rule);
            Ok(())
        }
        "deny" => {
            let rule = parse_simple_rule(value, false)?;
            controller.rules.push(rule);
            Ok(())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Parse simple rule format (e.g., "c 1:3 rwm")
fn parse_simple_rule(s: &str, allow: bool) -> Result<DeviceRule, CgroupError> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(CgroupError::InvalidPath);
    }

    let device_type = parts[0].chars().next()
        .and_then(DeviceType::from_char)
        .ok_or(CgroupError::InvalidPath)?;

    let dev_parts: Vec<&str> = parts[1].split(':').collect();
    if dev_parts.len() != 2 {
        return Err(CgroupError::InvalidPath);
    }

    let major = if dev_parts[0] == "*" {
        None
    } else {
        Some(dev_parts[0].parse().map_err(|_| CgroupError::InvalidPath)?)
    };

    let minor = if dev_parts[1] == "*" {
        None
    } else {
        Some(dev_parts[1].parse().map_err(|_| CgroupError::InvalidPath)?)
    };

    let access = Access::from_str(parts[2]);

    Ok(DeviceRule {
        allow,
        device_type,
        major,
        minor,
        access,
    })
}

/// Check device access for a process
pub fn check_device_access(
    controller: &DevicesController,
    device_type: DeviceType,
    major: u32,
    minor: u32,
    access: Access,
) -> bool {
    controller.check_access(device_type, major, minor, access)
}

/// Apply device rules to a process
pub fn apply_to_process(pid: u32, controller: &DevicesController) -> Result<(), CgroupError> {
    crate::kprintln!("[CGROUPS] Process {} device rules: {} rules",
        pid, controller.rules.len());
    Ok(())
}
