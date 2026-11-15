// ===============================================================================
// BUILDOS KERNEL - BPF HELPERS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! BPF Helper Functions
//!
//! Provides kernel functionality to BPF programs through a stable ABI.

use super::BpfError;
use core::sync::atomic::{AtomicU64, Ordering};

/// Helper function IDs
/// Names match Linux BPF ABI for compatibility
#[allow(non_upper_case_globals)]
pub mod helpers {
    /// Unspecified
    pub const BPF_FUNC_unspec: i32 = 0;
    /// Map lookup
    pub const BPF_FUNC_map_lookup_elem: i32 = 1;
    /// Map update
    pub const BPF_FUNC_map_update_elem: i32 = 2;
    /// Map delete
    pub const BPF_FUNC_map_delete_elem: i32 = 3;
    /// Probe read
    pub const BPF_FUNC_probe_read: i32 = 4;
    /// Get current time (ktime)
    pub const BPF_FUNC_ktime_get_ns: i32 = 5;
    /// Print to trace
    pub const BPF_FUNC_trace_printk: i32 = 6;
    /// Get pseudo-random number
    pub const BPF_FUNC_get_prandom_u32: i32 = 7;
    /// Get SMP processor ID
    pub const BPF_FUNC_get_smp_processor_id: i32 = 8;
    /// Skb store bytes
    pub const BPF_FUNC_skb_store_bytes: i32 = 9;
    /// L3 checksum replace
    pub const BPF_FUNC_l3_csum_replace: i32 = 10;
    /// L4 checksum replace
    pub const BPF_FUNC_l4_csum_replace: i32 = 11;
    /// Tail call
    pub const BPF_FUNC_tail_call: i32 = 12;
    /// Clone redirect
    pub const BPF_FUNC_clone_redirect: i32 = 13;
    /// Get current PID/TID
    pub const BPF_FUNC_get_current_pid_tgid: i32 = 14;
    /// Get current UID/GID
    pub const BPF_FUNC_get_current_uid_gid: i32 = 15;
    /// Get current comm
    pub const BPF_FUNC_get_current_comm: i32 = 16;
    /// Get cgroup classid
    pub const BPF_FUNC_get_cgroup_classid: i32 = 17;
    /// Skb vlan push
    pub const BPF_FUNC_skb_vlan_push: i32 = 18;
    /// Skb vlan pop
    pub const BPF_FUNC_skb_vlan_pop: i32 = 19;
    /// Skb get tunnel key
    pub const BPF_FUNC_skb_get_tunnel_key: i32 = 20;
    /// Skb set tunnel key
    pub const BPF_FUNC_skb_set_tunnel_key: i32 = 21;
    /// Perf event read
    pub const BPF_FUNC_perf_event_read: i32 = 22;
    /// Redirect
    pub const BPF_FUNC_redirect: i32 = 23;
    /// Get route realm
    pub const BPF_FUNC_get_route_realm: i32 = 24;
    /// Perf event output
    pub const BPF_FUNC_perf_event_output: i32 = 25;
    /// Skb load bytes
    pub const BPF_FUNC_skb_load_bytes: i32 = 26;
    /// Get stackid
    pub const BPF_FUNC_get_stackid: i32 = 27;
    /// Csum diff
    pub const BPF_FUNC_csum_diff: i32 = 28;
    /// Skb get tunnel opt
    pub const BPF_FUNC_skb_get_tunnel_opt: i32 = 29;
    /// Skb set tunnel opt
    pub const BPF_FUNC_skb_set_tunnel_opt: i32 = 30;
    /// Skb change proto
    pub const BPF_FUNC_skb_change_proto: i32 = 31;
    /// Skb change type
    pub const BPF_FUNC_skb_change_type: i32 = 32;
    /// Skb under cgroup
    pub const BPF_FUNC_skb_under_cgroup: i32 = 33;
    /// Get hash recalc
    pub const BPF_FUNC_get_hash_recalc: i32 = 34;
    /// Get current task
    pub const BPF_FUNC_get_current_task: i32 = 35;
    /// Probe write user
    pub const BPF_FUNC_probe_write_user: i32 = 36;
    /// Current task under cgroup
    pub const BPF_FUNC_current_task_under_cgroup: i32 = 37;
    /// Skb change tail
    pub const BPF_FUNC_skb_change_tail: i32 = 38;
    /// Skb pull data
    pub const BPF_FUNC_skb_pull_data: i32 = 39;
    /// Csum update
    pub const BPF_FUNC_csum_update: i32 = 40;
    /// Set hash invalid
    pub const BPF_FUNC_set_hash_invalid: i32 = 41;
    /// Get numa node ID
    pub const BPF_FUNC_get_numa_node_id: i32 = 42;
    /// Skb change head
    pub const BPF_FUNC_skb_change_head: i32 = 43;
    /// XDP adjust head
    pub const BPF_FUNC_xdp_adjust_head: i32 = 44;
    /// Probe read str
    pub const BPF_FUNC_probe_read_str: i32 = 45;
    /// Get socket cookie
    pub const BPF_FUNC_get_socket_cookie: i32 = 46;
    /// Get socket uid
    pub const BPF_FUNC_get_socket_uid: i32 = 47;
    /// Set hash
    pub const BPF_FUNC_set_hash: i32 = 48;
    /// Setsockopt
    pub const BPF_FUNC_setsockopt: i32 = 49;
    /// Skb adjust room
    pub const BPF_FUNC_skb_adjust_room: i32 = 50;
    /// Redirect map
    pub const BPF_FUNC_redirect_map: i32 = 51;
    /// Sk redirect map
    pub const BPF_FUNC_sk_redirect_map: i32 = 52;
    /// Sock map update
    pub const BPF_FUNC_sock_map_update: i32 = 53;
    /// XDP adjust meta
    pub const BPF_FUNC_xdp_adjust_meta: i32 = 54;
    /// Perf event read value
    pub const BPF_FUNC_perf_event_read_value: i32 = 55;
    /// Perf prog read value
    pub const BPF_FUNC_perf_prog_read_value: i32 = 56;
    /// Getsockopt
    pub const BPF_FUNC_getsockopt: i32 = 57;
    /// Override return
    pub const BPF_FUNC_override_return: i32 = 58;
    /// Sock ops cb flags set
    pub const BPF_FUNC_sock_ops_cb_flags_set: i32 = 59;
    /// Msg redirect map
    pub const BPF_FUNC_msg_redirect_map: i32 = 60;
    /// Msg apply bytes
    pub const BPF_FUNC_msg_apply_bytes: i32 = 61;
    /// Msg cork bytes
    pub const BPF_FUNC_msg_cork_bytes: i32 = 62;
    /// Msg pull data
    pub const BPF_FUNC_msg_pull_data: i32 = 63;
    /// Bind
    pub const BPF_FUNC_bind: i32 = 64;
    /// XDP adjust tail
    pub const BPF_FUNC_xdp_adjust_tail: i32 = 65;
    /// Skb get xfrm state
    pub const BPF_FUNC_skb_get_xfrm_state: i32 = 66;
    /// Get stack
    pub const BPF_FUNC_get_stack: i32 = 67;
    /// Skb load bytes relative
    pub const BPF_FUNC_skb_load_bytes_relative: i32 = 68;
    /// Fib lookup
    pub const BPF_FUNC_fib_lookup: i32 = 69;
    /// Sock hash update
    pub const BPF_FUNC_sock_hash_update: i32 = 70;
    /// Msg redirect hash
    pub const BPF_FUNC_msg_redirect_hash: i32 = 71;
    /// Sk redirect hash
    pub const BPF_FUNC_sk_redirect_hash: i32 = 72;
    /// Lwt push encap
    pub const BPF_FUNC_lwt_push_encap: i32 = 73;
    /// Lwt seg6 store bytes
    pub const BPF_FUNC_lwt_seg6_store_bytes: i32 = 74;
    /// Lwt seg6 adjust srh
    pub const BPF_FUNC_lwt_seg6_adjust_srh: i32 = 75;
    /// Lwt seg6 action
    pub const BPF_FUNC_lwt_seg6_action: i32 = 76;
    /// Rc repeat
    pub const BPF_FUNC_rc_repeat: i32 = 77;
    /// Rc keydown
    pub const BPF_FUNC_rc_keydown: i32 = 78;
    /// Skb cgroup id
    pub const BPF_FUNC_skb_cgroup_id: i32 = 79;
    /// Get current cgroup id
    pub const BPF_FUNC_get_current_cgroup_id: i32 = 80;
    /// Get local storage
    pub const BPF_FUNC_get_local_storage: i32 = 81;
    /// Sk select reuseport
    pub const BPF_FUNC_sk_select_reuseport: i32 = 82;
    /// Skb ancestor cgroup id
    pub const BPF_FUNC_skb_ancestor_cgroup_id: i32 = 83;
    /// Sk lookup tcp
    pub const BPF_FUNC_sk_lookup_tcp: i32 = 84;
    /// Sk lookup udp
    pub const BPF_FUNC_sk_lookup_udp: i32 = 85;
    /// Sk release
    pub const BPF_FUNC_sk_release: i32 = 86;
    /// Map push elem
    pub const BPF_FUNC_map_push_elem: i32 = 87;
    /// Map pop elem
    pub const BPF_FUNC_map_pop_elem: i32 = 88;
    /// Map peek elem
    pub const BPF_FUNC_map_peek_elem: i32 = 89;
    /// Msg push data
    pub const BPF_FUNC_msg_push_data: i32 = 90;
    /// Msg pop data
    pub const BPF_FUNC_msg_pop_data: i32 = 91;
    /// Rc pointer rel
    pub const BPF_FUNC_rc_pointer_rel: i32 = 92;
    /// Spin lock
    pub const BPF_FUNC_spin_lock: i32 = 93;
    /// Spin unlock
    pub const BPF_FUNC_spin_unlock: i32 = 94;
    /// Sk fullsock
    pub const BPF_FUNC_sk_fullsock: i32 = 95;
    /// Tcp sock
    pub const BPF_FUNC_tcp_sock: i32 = 96;
    /// Skb ecn set ce
    pub const BPF_FUNC_skb_ecn_set_ce: i32 = 97;
    /// Get listener sock
    pub const BPF_FUNC_get_listener_sock: i32 = 98;
    /// Skc lookup tcp
    pub const BPF_FUNC_skc_lookup_tcp: i32 = 99;
    /// Tcp check syncookie
    pub const BPF_FUNC_tcp_check_syncookie: i32 = 100;
    /// Ring buffer reserve
    pub const BPF_FUNC_ringbuf_reserve: i32 = 131;
    /// Ring buffer submit
    pub const BPF_FUNC_ringbuf_submit: i32 = 132;
    /// Ring buffer discard
    pub const BPF_FUNC_ringbuf_discard: i32 = 133;
    /// Ring buffer output
    pub const BPF_FUNC_ringbuf_output: i32 = 134;
    /// Ring buffer query
    pub const BPF_FUNC_ringbuf_query: i32 = 135;
}

/// Monotonic time counter
static KTIME_NS: AtomicU64 = AtomicU64::new(0);

/// Simple PRNG state
static PRNG_STATE: AtomicU64 = AtomicU64::new(0x12345678_9ABCDEF0);

/// Call a helper function
pub fn call_helper(
    helper_id: i32,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> Result<u64, BpfError> {
    match helper_id {
        helpers::BPF_FUNC_unspec => Ok(0),

        helpers::BPF_FUNC_map_lookup_elem => {
            // Would lookup in map
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_map_update_elem => {
            // Would update map element
            let _ = (arg1, arg2, arg3, arg4);
            Ok(0)
        }

        helpers::BPF_FUNC_map_delete_elem => {
            // Would delete from map
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_probe_read => {
            // Would read from arbitrary kernel address
            let _ = (arg1, arg2, arg3);
            Ok(0)
        }

        helpers::BPF_FUNC_ktime_get_ns => {
            // Get monotonic time
            Ok(KTIME_NS.fetch_add(1000, Ordering::Relaxed))
        }

        helpers::BPF_FUNC_trace_printk => {
            // Would print to trace buffer
            let _ = (arg1, arg2, arg3, arg4, arg5);
            Ok(0)
        }

        helpers::BPF_FUNC_get_prandom_u32 => {
            // Simple xorshift PRNG
            let mut state = PRNG_STATE.load(Ordering::Relaxed);
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            PRNG_STATE.store(state, Ordering::Relaxed);
            Ok(state as u32 as u64)
        }

        helpers::BPF_FUNC_get_smp_processor_id => {
            // Would return current CPU
            Ok(0)
        }

        helpers::BPF_FUNC_tail_call => {
            // Would perform tail call
            let _ = (arg1, arg2, arg3);
            Err(BpfError::InvalidHelper)
        }

        helpers::BPF_FUNC_get_current_pid_tgid => {
            // Would return PID/TGID
            Ok(0x0001_0001) // PID=1, TGID=1
        }

        helpers::BPF_FUNC_get_current_uid_gid => {
            // Would return UID/GID
            Ok(0x0000_0000) // UID=0, GID=0
        }

        helpers::BPF_FUNC_get_current_comm => {
            // Would copy current task comm to buffer
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_get_numa_node_id => {
            Ok(0)
        }

        helpers::BPF_FUNC_redirect => {
            // Would redirect packet
            let _ = (arg1, arg2);
            Ok(3) // XDP_TX
        }

        helpers::BPF_FUNC_redirect_map => {
            // Would redirect via map
            let _ = (arg1, arg2, arg3);
            Ok(4) // XDP_REDIRECT
        }

        helpers::BPF_FUNC_perf_event_output => {
            // Would output to perf buffer
            let _ = (arg1, arg2, arg3, arg4, arg5);
            Ok(0)
        }

        helpers::BPF_FUNC_get_stackid => {
            // Would get stack ID
            let _ = (arg1, arg2, arg3);
            Ok(0)
        }

        helpers::BPF_FUNC_get_stack => {
            // Would get stack trace
            let _ = (arg1, arg2, arg3, arg4);
            Ok(0)
        }

        helpers::BPF_FUNC_get_current_task => {
            // Would return current task pointer
            Ok(0)
        }

        helpers::BPF_FUNC_get_current_cgroup_id => {
            // Would return current cgroup ID
            Ok(1)
        }

        helpers::BPF_FUNC_ringbuf_reserve => {
            // Would reserve ring buffer space
            let _ = (arg1, arg2, arg3);
            Ok(0)
        }

        helpers::BPF_FUNC_ringbuf_submit => {
            // Would submit ring buffer entry
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_ringbuf_discard => {
            // Would discard ring buffer entry
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_ringbuf_output => {
            // Would output to ring buffer
            let _ = (arg1, arg2, arg3, arg4);
            Ok(0)
        }

        helpers::BPF_FUNC_map_push_elem => {
            // Would push to queue/stack
            let _ = (arg1, arg2, arg3);
            Ok(0)
        }

        helpers::BPF_FUNC_map_pop_elem => {
            // Would pop from queue/stack
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_map_peek_elem => {
            // Would peek queue/stack
            let _ = (arg1, arg2);
            Ok(0)
        }

        helpers::BPF_FUNC_spin_lock => {
            // Would acquire spin lock
            let _ = arg1;
            Ok(0)
        }

        helpers::BPF_FUNC_spin_unlock => {
            // Would release spin lock
            let _ = arg1;
            Ok(0)
        }

        _ => Err(BpfError::InvalidHelper),
    }
}
