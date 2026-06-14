//! Capability-effect registry for ambient runtime surfaces.

pub const CONSOLE: &str = "Console";
pub const FILE_SYSTEM: &str = "FileSystem";
pub const NETWORK: &str = "Network";
pub const PROCESS: &str = "Process";
pub const ENVIRONMENT: &str = "Environment";
pub const CLOCK: &str = "Clock";
pub const FOREIGN: &str = "Foreign";
pub const GPU: &str = "Gpu";

const CAPABILITY_EFFECTS: &[&str] = &[
    CONSOLE,
    FILE_SYSTEM,
    NETWORK,
    PROCESS,
    ENVIRONMENT,
    CLOCK,
    FOREIGN,
    GPU,
];

pub fn capability_effect_names() -> &'static [&'static str] {
    CAPABILITY_EFFECTS
}

pub fn is_capability_effect(name: &str) -> bool {
    CAPABILITY_EFFECTS.contains(&name)
}

pub fn capability_effect_for_call(name: &str) -> Option<&'static str> {
    match name {
        "println" | "print" | "read_line" | "read_all" | "stdin_is_pipe" => Some(CONSOLE),
        "read_file" | "write_file" | "file_exists" | "read_bytes" | "write_bytes"
        | "append_file" | "list_dir" | "is_dir" | "file_size" => Some(FILE_SYSTEM),
        "tcp_connect" | "tcp_send" | "tcp_recv" | "tcp_close" => Some(NETWORK),
        "exit" | "process_exit" => Some(PROCESS),
        "getenv" | "args_count" | "args_get" => Some(ENVIRONMENT),
        "clock_ms" | "time_unix" => Some(CLOCK),
        "quanta_vk_init" | "quanta_vk_load_shader_file" | "quanta_vk_run_compute"
        | "quanta_vk_shutdown" | "quanta_vk_create_graphics_pipeline"
        | "quanta_vk_set_push_constant_f32" | "quanta_vk_draw_frame"
        | "quanta_vk_should_close" | "quanta_vk_request_close" | "quanta_vk_device_name" => {
            Some(GPU)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ambient_runtime_calls_to_capability_effects() {
        assert_eq!(capability_effect_for_call("read_file"), Some("FileSystem"));
        assert_eq!(capability_effect_for_call("write_file"), Some("FileSystem"));
        assert_eq!(capability_effect_for_call("list_dir"), Some("FileSystem"));
        assert_eq!(capability_effect_for_call("tcp_connect"), Some("Network"));
        assert_eq!(capability_effect_for_call("process_exit"), Some("Process"));
        assert_eq!(capability_effect_for_call("getenv"), Some("Environment"));
        assert_eq!(capability_effect_for_call("clock_ms"), Some("Clock"));
        assert_eq!(capability_effect_for_call("quanta_vk_init"), Some("Gpu"));
        assert_eq!(capability_effect_for_call("sqrt"), None);
    }

    #[test]
    fn lists_stable_capability_effect_names() {
        assert!(capability_effect_names().contains(&"Console"));
        assert!(capability_effect_names().contains(&"FileSystem"));
        assert!(capability_effect_names().contains(&"Network"));
        assert!(capability_effect_names().contains(&"Process"));
        assert!(capability_effect_names().contains(&"Environment"));
        assert!(capability_effect_names().contains(&"Clock"));
        assert!(capability_effect_names().contains(&"Foreign"));
        assert!(capability_effect_names().contains(&"Gpu"));
    }
}
