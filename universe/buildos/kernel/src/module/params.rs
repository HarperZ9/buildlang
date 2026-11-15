// ===============================================================================
// BUILDOS KERNEL - MODULE PARAMETERS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Module Parameters
//!
//! Provides module parameter parsing and management.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{ModuleError, ModuleParam, ParamType};

/// Parse a parameter value from string
pub fn parse_value(param_type: ParamType, value: &str) -> Result<ParamValue, ModuleError> {
    match param_type {
        ParamType::Bool => {
            let v = match value.to_lowercase().as_str() {
                "1" | "y" | "yes" | "true" | "on" => true,
                "0" | "n" | "no" | "false" | "off" => false,
                _ => return Err(ModuleError::InvalidParameter(value.to_string())),
            };
            Ok(ParamValue::Bool(v))
        }

        ParamType::Int => {
            let v = parse_int(value)?;
            if v > i32::MAX as i64 || v < i32::MIN as i64 {
                return Err(ModuleError::InvalidParameter(value.to_string()));
            }
            Ok(ParamValue::Int(v as i32))
        }

        ParamType::UInt => {
            let v = parse_uint(value)?;
            if v > u32::MAX as u64 {
                return Err(ModuleError::InvalidParameter(value.to_string()));
            }
            Ok(ParamValue::UInt(v as u32))
        }

        ParamType::Long => {
            let v = parse_int(value)?;
            Ok(ParamValue::Long(v))
        }

        ParamType::ULong => {
            let v = parse_uint(value)?;
            Ok(ParamValue::ULong(v))
        }

        ParamType::String | ParamType::CharPtr => {
            Ok(ParamValue::String(value.to_string()))
        }

        ParamType::ByteArray => {
            let bytes = parse_byte_array(value)?;
            Ok(ParamValue::ByteArray(bytes))
        }
    }
}

/// Parsed parameter value
#[derive(Clone, Debug)]
pub enum ParamValue {
    Bool(bool),
    Int(i32),
    UInt(u32),
    Long(i64),
    ULong(u64),
    String(String),
    ByteArray(Vec<u8>),
}

impl ParamValue {
    /// Convert to string representation
    pub fn to_string(&self) -> String {
        match self {
            ParamValue::Bool(v) => if *v { "Y".to_string() } else { "N".to_string() },
            ParamValue::Int(v) => alloc::format!("{}", v),
            ParamValue::UInt(v) => alloc::format!("{}", v),
            ParamValue::Long(v) => alloc::format!("{}", v),
            ParamValue::ULong(v) => alloc::format!("{}", v),
            ParamValue::String(v) => v.clone(),
            ParamValue::ByteArray(v) => {
                let mut s = String::new();
                for (i, byte) in v.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    s.push_str(&alloc::format!("{:02x}", byte));
                }
                s
            }
        }
    }
}

/// Parse an integer value (supports hex, octal, binary)
fn parse_int(s: &str) -> Result<i64, ModuleError> {
    let s = s.trim();
    let negative = s.starts_with('-');
    let s = if negative || s.starts_with('+') { &s[1..] } else { s };

    let (base, s) = if s.starts_with("0x") || s.starts_with("0X") {
        (16, &s[2..])
    } else if s.starts_with("0o") || s.starts_with("0O") {
        (8, &s[2..])
    } else if s.starts_with("0b") || s.starts_with("0B") {
        (2, &s[2..])
    } else if s.starts_with('0') && s.len() > 1 {
        (8, &s[1..])
    } else {
        (10, s)
    };

    let value = i64::from_str_radix(s, base)
        .map_err(|_| ModuleError::InvalidParameter(s.to_string()))?;

    Ok(if negative { -value } else { value })
}

/// Parse an unsigned integer value
fn parse_uint(s: &str) -> Result<u64, ModuleError> {
    let s = s.trim();

    let (base, s) = if s.starts_with("0x") || s.starts_with("0X") {
        (16, &s[2..])
    } else if s.starts_with("0o") || s.starts_with("0O") {
        (8, &s[2..])
    } else if s.starts_with("0b") || s.starts_with("0B") {
        (2, &s[2..])
    } else if s.starts_with('0') && s.len() > 1 {
        (8, &s[1..])
    } else {
        (10, s)
    };

    u64::from_str_radix(s, base)
        .map_err(|_| ModuleError::InvalidParameter(s.to_string()))
}

/// Parse a byte array (comma-separated hex bytes)
fn parse_byte_array(s: &str) -> Result<Vec<u8>, ModuleError> {
    let mut bytes = Vec::new();

    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Remove optional 0x prefix
        let part = part.strip_prefix("0x")
            .or_else(|| part.strip_prefix("0X"))
            .unwrap_or(part);

        let byte = u8::from_str_radix(part, 16)
            .map_err(|_| ModuleError::InvalidParameter(part.to_string()))?;
        bytes.push(byte);
    }

    Ok(bytes)
}

/// Parse module parameters from string (format: "param1=value1 param2=value2")
pub fn parse_params(s: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();

    for part in s.split_whitespace() {
        if let Some((key, value)) = part.split_once('=') {
            params.push((key.to_string(), value.to_string()));
        }
    }

    params
}

/// Module parameter descriptor (for kernel module macros)
#[repr(C)]
pub struct ParamDescriptor {
    /// Parameter name
    pub name: &'static str,
    /// Parameter type
    pub param_type: ParamType,
    /// Pointer to variable
    pub variable: *mut (),
    /// Description
    pub description: &'static str,
    /// Permissions (for sysfs)
    pub perm: u16,
}

unsafe impl Sync for ParamDescriptor {}
unsafe impl Send for ParamDescriptor {}

/// Apply parameters to a module
pub fn apply_params(
    descriptors: &[ParamDescriptor],
    params: &[(String, String)],
) -> Result<(), ModuleError> {
    for (key, value) in params {
        let desc = descriptors.iter()
            .find(|d| d.name == key)
            .ok_or_else(|| ModuleError::InvalidParameter(key.clone()))?;

        let parsed = parse_value(desc.param_type, value)?;

        unsafe {
            match parsed {
                ParamValue::Bool(v) => {
                    *(desc.variable as *mut bool) = v;
                }
                ParamValue::Int(v) => {
                    *(desc.variable as *mut i32) = v;
                }
                ParamValue::UInt(v) => {
                    *(desc.variable as *mut u32) = v;
                }
                ParamValue::Long(v) => {
                    *(desc.variable as *mut i64) = v;
                }
                ParamValue::ULong(v) => {
                    *(desc.variable as *mut u64) = v;
                }
                ParamValue::String(v) => {
                    // Would need proper string handling
                    let _ = v;
                }
                ParamValue::ByteArray(v) => {
                    let _ = v;
                }
            }
        }
    }

    Ok(())
}

/// Convert module params to descriptors for display
pub fn params_to_descriptors(params: &[ModuleParam]) -> Vec<ParamInfo> {
    params.iter().map(|p| ParamInfo {
        name: p.name.clone(),
        param_type: p.param_type,
        value: p.value.clone(),
        description: p.description.clone(),
        perm: p.perm,
    }).collect()
}

/// Parameter info for display
#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub name: String,
    pub param_type: ParamType,
    pub value: String,
    pub description: String,
    pub perm: u16,
}

/// Format parameters for /sys/module/*/parameters
pub fn format_sysfs(params: &[ModuleParam]) -> String {
    let mut output = String::new();

    for param in params {
        output.push_str(&alloc::format!("{}\n", param.name));
    }

    output
}

/// Read parameter value (for /sys/module/*/parameters/*)
pub fn read_param(params: &[ModuleParam], name: &str) -> Option<String> {
    params.iter()
        .find(|p| p.name == name)
        .map(|p| p.value.clone())
}

/// Write parameter value (for /sys/module/*/parameters/*)
pub fn write_param(
    params: &mut [ModuleParam],
    name: &str,
    value: &str,
) -> Result<(), ModuleError> {
    let param = params.iter_mut()
        .find(|p| p.name == name)
        .ok_or_else(|| ModuleError::InvalidParameter(name.to_string()))?;

    // Validate new value
    let _ = parse_value(param.param_type, value)?;

    param.value = value.to_string();

    Ok(())
}

/// Permission flags for parameters
pub mod perm {
    pub const S_IRUSR: u16 = 0o400;
    pub const S_IWUSR: u16 = 0o200;
    pub const S_IRGRP: u16 = 0o040;
    pub const S_IWGRP: u16 = 0o020;
    pub const S_IROTH: u16 = 0o004;
    pub const S_IWOTH: u16 = 0o002;

    pub const S_IRUGO: u16 = S_IRUSR | S_IRGRP | S_IROTH;
    pub const S_IWUGO: u16 = S_IWUSR | S_IWGRP | S_IWOTH;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int() {
        assert_eq!(parse_int("42").unwrap(), 42);
        assert_eq!(parse_int("-42").unwrap(), -42);
        assert_eq!(parse_int("0x2a").unwrap(), 42);
        assert_eq!(parse_int("0o52").unwrap(), 42);
        assert_eq!(parse_int("0b101010").unwrap(), 42);
    }

    #[test]
    fn test_parse_bool() {
        assert!(matches!(parse_value(ParamType::Bool, "1").unwrap(), ParamValue::Bool(true)));
        assert!(matches!(parse_value(ParamType::Bool, "yes").unwrap(), ParamValue::Bool(true)));
        assert!(matches!(parse_value(ParamType::Bool, "0").unwrap(), ParamValue::Bool(false)));
        assert!(matches!(parse_value(ParamType::Bool, "no").unwrap(), ParamValue::Bool(false)));
    }

    #[test]
    fn test_parse_params() {
        let params = parse_params("debug=1 timeout=30");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ("debug".to_string(), "1".to_string()));
        assert_eq!(params[1], ("timeout".to_string(), "30".to_string()));
    }
}
