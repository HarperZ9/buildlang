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
        "println"
        | "print"
        | "read_line"
        | "read_all"
        | "stdin_is_pipe"
        | "build_print_i32"
        | "build_print_i64"
        | "build_print_f32"
        | "build_print_f64"
        | "build_print_bool"
        | "build_print_str"
        | "build_print_string"
        | "build_print_char"
        | "build_eprint_str"
        | "build_eprint_string"
        | "build_read_line"
        | "build_read_all"
        | "build_stdin_is_pipe" => Some(CONSOLE),
        "read_file" | "write_file" | "file_exists" | "read_bytes" | "write_bytes"
        | "append_file" | "list_dir" | "is_dir" | "file_size" | "build_read_file"
        | "build_write_file" | "build_file_exists" | "build_read_bytes" | "build_write_bytes"
        | "build_append_file" | "build_list_dir" | "build_is_dir" | "build_file_size" => {
            Some(FILE_SYSTEM)
        }
        "tcp_connect" | "tcp_send" | "tcp_recv" | "tcp_close" | "build_tcp_connect"
        | "build_tcp_send" | "build_tcp_recv" | "build_tcp_close" => Some(NETWORK),
        "exit" | "process_exit" | "build_exit" | "build_process_exit" => Some(PROCESS),
        "getenv" | "args_count" | "args_get" | "build_getenv" | "build_args_init"
        | "build_args_count" | "build_args_get" => Some(ENVIRONMENT),
        "clock_ms" | "time_unix" | "build_clock_ms" | "build_time_unix" => Some(CLOCK),
        "build_vk_init"
        | "build_vk_load_shader_file"
        | "build_vk_run_compute"
        | "build_vk_shutdown"
        | "build_vk_create_graphics_pipeline"
        | "build_vk_set_push_constant_f32"
        | "build_vk_draw_frame"
        | "build_vk_should_close"
        | "build_vk_request_close"
        | "build_vk_device_name"
        | "build_gfx_init"
        | "build_gfx_load_shader"
        | "build_gfx_create_pipeline"
        | "build_gfx_begin_frame"
        | "build_gfx_clear"
        | "build_gfx_draw"
        | "build_gfx_end_frame"
        | "build_gfx_should_close"
        | "build_gfx_shutdown" => Some(GPU),
        _ => None,
    }
}

pub fn capability_effect_for_macro(name: &str) -> Option<&'static str> {
    match name {
        "println" | "print" | "eprintln" | "eprint" | "dbg" | "debug" | "log" | "trace"
        | "warn" | "error" => Some(CONSOLE),
        "include" | "include_str" | "include_bytes" => Some(FILE_SYSTEM),
        "env" | "option_env" => Some(ENVIRONMENT),
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
        assert_eq!(capability_effect_for_call("build_vk_init"), Some("Gpu"));
        assert_eq!(
            capability_effect_for_call("build_read_file"),
            Some("FileSystem")
        );
        assert_eq!(
            capability_effect_for_call("build_tcp_connect"),
            Some("Network")
        );
        assert_eq!(
            capability_effect_for_call("build_process_exit"),
            Some("Process")
        );
        assert_eq!(
            capability_effect_for_call("build_getenv"),
            Some("Environment")
        );
        assert_eq!(capability_effect_for_call("build_clock_ms"), Some("Clock"));
        assert_eq!(capability_effect_for_call("build_gfx_init"), Some("Gpu"));
        assert_eq!(capability_effect_for_call("build_gfx_draw"), Some("Gpu"));
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

    #[test]
    fn maps_console_macros_to_console_capability() {
        assert_eq!(capability_effect_for_macro("println"), Some("Console"));
        assert_eq!(capability_effect_for_macro("eprintln"), Some("Console"));
        assert_eq!(capability_effect_for_macro("dbg"), Some("Console"));
        assert_eq!(capability_effect_for_macro("include"), Some("FileSystem"));
        assert_eq!(
            capability_effect_for_macro("include_str"),
            Some("FileSystem")
        );
        assert_eq!(
            capability_effect_for_macro("include_bytes"),
            Some("FileSystem")
        );
        assert_eq!(capability_effect_for_macro("env"), Some("Environment"));
        assert_eq!(
            capability_effect_for_macro("option_env"),
            Some("Environment")
        );
        assert_eq!(capability_effect_for_macro("format"), None);
        assert_eq!(capability_effect_for_macro("file"), None);
    }
}
