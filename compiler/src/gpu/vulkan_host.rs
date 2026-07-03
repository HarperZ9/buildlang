// ===============================================================================
// BUILDLANG GPU HOST - Vulkan compute dispatch (feature "gpu")
// ===============================================================================
// Copyright (c) 2026 Zain Dana Harper. BuildLang Fair-Source License v1.0.
// ===============================================================================
//
//! Layer B host: take a compute SPIR-V module (produced by the SPIR-V backend),
//! bind host-provided f32 buffers as StorageBuffers, dispatch on the physical
//! device, and read back the output. Compiled ONLY under `--features gpu`; the
//! default build never references `ash`.
//!
//! Scope: arbitrary ELEMENTWISE f32 kernels plus 2D-grid f32 kernels (matmul) —
//! any number of f32 StorageBuffers at descriptor set 0, bindings 0..N
//! (declaration order), plus an optional push-constant block for scalar params.
//! The grid is 1D (`gl_GlobalInvocationID.x`, one invocation per element) or 2D
//! (`.x`/`.y`, one invocation per output element). This same D1 dispatch path
//! also carries the 1D neighbor-access (stencil) kernel; see gpu/mod.rs's
//! run_stencil_cross_check. Still deliberately narrow (1D/2D, f32; no
//! reductions/shared memory): enough to run and cross-check elementwise +
//! matmul + 1D-stencil kernels on a real device, not a general Vulkan compute
//! framework.

use std::ffi::CStr;

use ash::vk;

/// A fatal error from the Vulkan host path. Kept as a String so the caller can
/// print it and fail the run without a heavyweight error type.
pub type HostResult<T> = Result<T, String>;

/// Probe for a Vulkan device that exposes a compute queue and return its name,
/// or `None` if no such instance/device could be created. Used by
/// `buildc doctor` and the device-gated tests.
pub fn probe_device() -> Option<String> {
    // Safety: standard ash entry/instance lifecycle; all handles dropped here.
    unsafe {
        let entry = ash::Entry::load().ok()?;
        let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
        let create = vk::InstanceCreateInfo::default().application_info(&app);
        let instance = entry.create_instance(&create, None).ok()?;
        let name = instance
            .enumerate_physical_devices()
            .ok()
            .and_then(|devices| {
                devices.into_iter().find(|&pd| {
                    instance
                        .get_physical_device_queue_family_properties(pd)
                        .iter()
                        .any(|p| p.queue_flags.contains(vk::QueueFlags::COMPUTE))
                })
            })
            .map(|pd| {
                let props = instance.get_physical_device_properties(pd);
                CStr::from_ptr(props.device_name.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            });
        instance.destroy_instance(None);
        name
    }
}

/// One f32 buffer bound to a compute kernel: its data (uploaded before
/// dispatch) and whether the kernel writes into it (the single writable buffer
/// is read back after the dispatch). Each buffer carries its OWN length -- the
/// host no longer assumes all buffers share one `n` (though Phase-1
/// elementwise kernels do use equal lengths).
pub struct BufferArg<'a> {
    pub data: &'a [f32],
    pub writable: bool,
}

/// The dispatch grid: how many invocations to launch, and in how many
/// dimensions. Phase 1 elementwise kernels use `D1` (one invocation per
/// element over `gl_GlobalInvocationID.x`); Phase 2 kernels that read
/// `gl_GlobalInvocationID.y` (matmul) use `D2` (one invocation per output
/// element over a 2D grid, x = column, y = row).
#[derive(Clone, Copy, Debug)]
pub enum Grid {
    /// 1D grid of `gx` invocations.
    D1(usize),
    /// 2D grid of `gx` columns by `gy` rows.
    D2 { gx: usize, gy: usize },
}

/// Dispatch a compute kernel over `buffers` (each an f32 StorageBuffer at
/// descriptor set 0, bindings 0..N, in declaration order) with `push_constants`
/// bytes pushed to the pipeline's push-constant range, and return the readback
/// of the single writable buffer.
/// Validate matmul buffer lengths against the shape `(m, k, n)` BEFORE any
/// device work: A must be `m*k`, B must be `k*n`, C must be `m*n` f32. Returns a
/// clear, dimension-named error on a mismatch. Pure (no Vulkan), so a shape bug
/// is caught deterministically on any machine -- a dispatch on inconsistent
/// buffers would otherwise read/write out of bounds on the device.
///
/// The dimensions need NOT be multiples of the workgroup size. The matmul kernel
/// now carries an in-body bounds guard (`if i < m && j < n { ... }`), so the
/// extra invocations the host `div_ceil`s over the workgroup for a non-multiple
/// dimension simply NO-OP -- they never read or write past the exactly-sized
/// `a`/`b`/`c` buffers. (Before the SPIR-V backend could emit a loop nested in a
/// selection, the kernel was unguarded and this function had to REFUSE
/// non-multiple dims to avoid an out-of-bounds device access; that constraint is
/// now lifted.) Only the internal length consistency (A = m*k, B = k*n, C = m*n)
/// and a non-zero workgroup remain load-bearing here.
pub fn validate_matmul_shapes(
    m: usize,
    k: usize,
    n: usize,
    len_a: usize,
    len_b: usize,
    len_c: usize,
    local_size: (u32, u32),
) -> HostResult<()> {
    if len_a != m * k {
        return Err(format!(
            "matmul shape mismatch: A has {len_a} elements but m*k = {m}*{k} = {}",
            m * k
        ));
    }
    if len_b != k * n {
        return Err(format!(
            "matmul shape mismatch: B has {len_b} elements but k*n = {k}*{n} = {}",
            k * n
        ));
    }
    if len_c != m * n {
        return Err(format!(
            "matmul shape mismatch: C has {len_c} elements but m*n = {m}*{n} = {}",
            m * n
        ));
    }
    // A non-zero workgroup is still required so the caller's per-axis `div_ceil`
    // is well-defined. Evenness is NOT required: the in-body guard makes the
    // over-launched invocations safe.
    let (lx, ly) = (local_size.0 as usize, local_size.1 as usize);
    if lx == 0 || ly == 0 {
        return Err(format!(
            "matmul workgroup size must be non-zero on both axes; got ({lx}, {ly})"
        ));
    }
    Ok(())
}

///
/// `spirv` is the compiled module, `entry` the entry-point name, `grid` the
/// dispatch grid (1D for elementwise, 2D for matmul), `local_size` the kernel's
/// declared workgroup (x, y) size (used to compute the group counts, div_ceil
/// per axis). Generalizes the old single-shape `dispatch_vec_add`: arbitrary
/// buffer count, arbitrary per-buffer length, an optional push-constant block,
/// the writable buffer identified by its flag rather than "the last binding",
/// and now a 1D or 2D grid.
pub fn dispatch_compute(
    spirv: &[u32],
    entry: &str,
    buffers: &[BufferArg<'_>],
    push_constants: &[u8],
    grid: Grid,
    local_size: (u32, u32),
) -> HostResult<Vec<f32>> {
    let writable: Vec<usize> = buffers
        .iter()
        .enumerate()
        .filter(|(_, b)| b.writable)
        .map(|(i, _)| i)
        .collect();
    let output_index = match writable.as_slice() {
        [i] => *i,
        [] => return Err("dispatch_compute: no writable output buffer".to_string()),
        _ => {
            return Err(
                "dispatch_compute: exactly one writable output buffer is supported".to_string(),
            )
        }
    };
    unsafe {
        dispatch_inner(
            spirv,
            entry,
            buffers,
            push_constants,
            output_index,
            grid,
            local_size,
        )
    }
}

unsafe fn dispatch_inner(
    spirv: &[u32],
    entry: &str,
    buffers: &[BufferArg<'_>],
    push_constants: &[u8],
    output_index: usize,
    grid: Grid,
    local_size: (u32, u32),
) -> HostResult<Vec<f32>> {
    let buffer_count = buffers.len();
    let entry_loader = ash::Entry::load().map_err(|e| format!("load Vulkan loader: {e}"))?;

    // --- Instance -----------------------------------------------------------
    let app = vk::ApplicationInfo::default()
        .application_name(CStr::from_bytes_with_nul(b"buildc\0").unwrap())
        .api_version(vk::API_VERSION_1_1);
    let inst_ci = vk::InstanceCreateInfo::default().application_info(&app);
    let instance = entry_loader
        .create_instance(&inst_ci, None)
        .map_err(|e| format!("create Vulkan instance: {e}"))?;

    let result = (|| {
        // --- Physical device + compute queue family -------------------------
        let physical = instance
            .enumerate_physical_devices()
            .map_err(|e| format!("enumerate devices: {e}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "no Vulkan physical device".to_string())?;

        let queue_family = instance
            .get_physical_device_queue_family_properties(physical)
            .into_iter()
            .enumerate()
            .find(|(_, p)| p.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .map(|(i, _)| i as u32)
            .ok_or_else(|| "no compute queue family".to_string())?;

        let mem_props = instance.get_physical_device_memory_properties(physical);

        // --- Logical device + queue -----------------------------------------
        let priorities = [1.0f32];
        let queue_ci = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities);
        let queue_cis = [queue_ci];
        let device_ci = vk::DeviceCreateInfo::default().queue_create_infos(&queue_cis);
        let device = instance
            .create_device(physical, &device_ci, None)
            .map_err(|e| format!("create logical device: {e}"))?;
        let queue = device.get_device_queue(queue_family, 0);

        let dispatch = || -> HostResult<Vec<f32>> {
            // Per-buffer byte length: no single-`n` assumption. Each buffer is
            // sized to its own data (min 4 bytes so a zero-length buffer is
            // still a valid allocation).
            let out_len = buffers[output_index].data.len();

            // --- Allocate buffers + memory ----------------------------------
            let mut vk_buffers = Vec::with_capacity(buffer_count);
            let mut memories = Vec::with_capacity(buffer_count);
            for arg in buffers.iter() {
                let bytes = (arg.data.len().max(1) * std::mem::size_of::<f32>()) as vk::DeviceSize;
                let (buf, mem) = create_host_visible_buffer(&device, &mem_props, bytes)?;
                vk_buffers.push(buf);
                memories.push(mem);
            }

            // Upload every buffer's data. The writable output is uploaded too
            // (its initial contents are the caller's, typically zeros), so no
            // separate zeroing pass is needed.
            for (i, arg) in buffers.iter().enumerate() {
                if !arg.data.is_empty() {
                    write_buffer_f32(&device, memories[i], arg.data)?;
                }
            }
            let buffers = vk_buffers;

            // --- Descriptor set layout (buffer_count storage buffers) -------
            let bindings: Vec<vk::DescriptorSetLayoutBinding> = (0..buffer_count)
                .map(|i| {
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                })
                .collect();
            let dsl_ci = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            let dsl = device
                .create_descriptor_set_layout(&dsl_ci, None)
                .map_err(|e| format!("create descriptor set layout: {e}"))?;

            // --- Pipeline layout + shader module + pipeline -----------------
            // Wire a push-constant range covering the caller's bytes (visible to
            // the compute stage) when the kernel has scalar params; otherwise
            // the layout has no push-constant range.
            let set_layouts = [dsl];
            let pc_ranges = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .offset(0)
                .size(push_constants.len() as u32)];
            let mut pl_ci = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
            if !push_constants.is_empty() {
                pl_ci = pl_ci.push_constant_ranges(&pc_ranges);
            }
            let pipeline_layout = device
                .create_pipeline_layout(&pl_ci, None)
                .map_err(|e| format!("create pipeline layout: {e}"))?;

            let sm_ci = vk::ShaderModuleCreateInfo::default().code(spirv);
            let shader = device
                .create_shader_module(&sm_ci, None)
                .map_err(|e| format!("create shader module: {e}"))?;

            let entry_c = std::ffi::CString::new(entry).map_err(|_| {
                format!("entry point name contains an interior NUL byte: {entry:?}")
            })?;
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(shader)
                .name(&entry_c);
            let cp_ci = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(pipeline_layout);
            let pipelines = device
                .create_compute_pipelines(vk::PipelineCache::null(), &[cp_ci], None)
                .map_err(|(_, e)| format!("create compute pipeline: {e}"))?;
            let pipeline = pipelines[0];

            // --- Descriptor pool + set --------------------------------------
            let pool_sizes = [vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(buffer_count as u32)];
            let dp_ci = vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&pool_sizes);
            let descriptor_pool = device
                .create_descriptor_pool(&dp_ci, None)
                .map_err(|e| format!("create descriptor pool: {e}"))?;

            let alloc_layouts = [dsl];
            let ds_ai = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&alloc_layouts);
            let descriptor_set = device
                .allocate_descriptor_sets(&ds_ai)
                .map_err(|e| format!("allocate descriptor set: {e}"))?[0];

            let buffer_infos: Vec<[vk::DescriptorBufferInfo; 1]> = buffers
                .iter()
                .map(|&b| {
                    [vk::DescriptorBufferInfo::default()
                        .buffer(b)
                        .offset(0)
                        .range(vk::WHOLE_SIZE)]
                })
                .collect();
            let writes: Vec<vk::WriteDescriptorSet> = buffer_infos
                .iter()
                .enumerate()
                .map(|(i, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(info)
                })
                .collect();
            device.update_descriptor_sets(&writes, &[]);

            // --- Command buffer: bind + dispatch ----------------------------
            let cp_ci = vk::CommandPoolCreateInfo::default().queue_family_index(queue_family);
            let command_pool = device
                .create_command_pool(&cp_ci, None)
                .map_err(|e| format!("create command pool: {e}"))?;
            let cb_ai = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd = device
                .allocate_command_buffers(&cb_ai)
                .map_err(|e| format!("allocate command buffer: {e}"))?[0];

            let begin = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(cmd, &begin)
                .map_err(|e| format!("begin command buffer: {e}"))?;
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
            // Push the scalar block (e.g. `alpha`) before dispatch, if any.
            if !push_constants.is_empty() {
                device.cmd_push_constants(
                    cmd,
                    pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    push_constants,
                );
            }
            // Group counts mirror the kernel's workgroup size: div_ceil per axis
            // so every element is covered. A 1D grid launches one row of groups.
            let (lx, ly) = (local_size.0.max(1), local_size.1.max(1));
            let (groups_x, groups_y) = match grid {
                Grid::D1(gx) => ((gx as u32).div_ceil(lx), 1),
                Grid::D2 { gx, gy } => ((gx as u32).div_ceil(lx), (gy as u32).div_ceil(ly)),
            };
            device.cmd_dispatch(cmd, groups_x, groups_y, 1);
            device
                .end_command_buffer(cmd)
                .map_err(|e| format!("end command buffer: {e}"))?;

            // --- Submit + wait ---------------------------------------------
            let cmds = [cmd];
            let submit = vk::SubmitInfo::default().command_buffers(&cmds);
            let fence_ci = vk::FenceCreateInfo::default();
            let fence = device
                .create_fence(&fence_ci, None)
                .map_err(|e| format!("create fence: {e}"))?;
            device
                .queue_submit(queue, &[submit], fence)
                .map_err(|e| format!("queue submit: {e}"))?;
            device
                .wait_for_fences(&[fence], true, u64::MAX)
                .map_err(|e| format!("wait for fence: {e}"))?;

            // --- Readback (the single writable output buffer) ---------------
            let out = read_buffer_f32(&device, memories[output_index], out_len)?;

            // --- Teardown (device-scoped objects) ---------------------------
            device.destroy_fence(fence, None);
            device.destroy_command_pool(command_pool, None);
            device.destroy_descriptor_pool(descriptor_pool, None);
            device.destroy_pipeline(pipeline, None);
            device.destroy_shader_module(shader, None);
            device.destroy_pipeline_layout(pipeline_layout, None);
            device.destroy_descriptor_set_layout(dsl, None);
            for &b in &buffers {
                device.destroy_buffer(b, None);
            }
            for &m in &memories {
                device.free_memory(m, None);
            }
            Ok(out)
        };

        let out = dispatch();
        device.destroy_device(None);
        out
    })();

    instance.destroy_instance(None);
    result
}

/// Create a host-visible, host-coherent StorageBuffer of `byte_len` bytes.
unsafe fn create_host_visible_buffer(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    byte_len: vk::DeviceSize,
) -> HostResult<(vk::Buffer, vk::DeviceMemory)> {
    let buf_ci = vk::BufferCreateInfo::default()
        .size(byte_len)
        .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = device
        .create_buffer(&buf_ci, None)
        .map_err(|e| format!("create buffer: {e}"))?;
    let req = device.get_buffer_memory_requirements(buffer);

    let mem_type = find_memory_type(
        mem_props,
        req.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .ok_or_else(|| "no host-visible coherent memory type".to_string())?;

    let alloc = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(mem_type);
    let memory = device
        .allocate_memory(&alloc, None)
        .map_err(|e| format!("allocate memory: {e}"))?;
    device
        .bind_buffer_memory(buffer, memory, 0)
        .map_err(|e| format!("bind buffer memory: {e}"))?;
    Ok((buffer, memory))
}

unsafe fn write_buffer_f32(
    device: &ash::Device,
    memory: vk::DeviceMemory,
    data: &[f32],
) -> HostResult<()> {
    let byte_len = std::mem::size_of_val(data) as vk::DeviceSize;
    let ptr = device
        .map_memory(memory, 0, byte_len, vk::MemoryMapFlags::empty())
        .map_err(|e| format!("map memory: {e}"))? as *mut f32;
    std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    device.unmap_memory(memory);
    Ok(())
}

unsafe fn read_buffer_f32(
    device: &ash::Device,
    memory: vk::DeviceMemory,
    n: usize,
) -> HostResult<Vec<f32>> {
    let byte_len = (n * std::mem::size_of::<f32>()) as vk::DeviceSize;
    let ptr = device
        .map_memory(memory, 0, byte_len, vk::MemoryMapFlags::empty())
        .map_err(|e| format!("map memory for readback: {e}"))? as *const f32;
    let mut out = vec![0.0f32; n];
    std::ptr::copy_nonoverlapping(ptr, out.as_mut_ptr(), n);
    device.unmap_memory(memory);
    Ok(out)
}

fn find_memory_type(
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    (0..mem_props.memory_type_count).find(|&i| {
        let supported = (type_bits & (1 << i)) != 0;
        let has_flags = mem_props.memory_types[i as usize]
            .property_flags
            .contains(flags);
        supported && has_flags
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 2D workgroup size the matmul cross-check uses; kept here so the shape
    /// tests exercise the SAME evenness contract the dispatch path enforces.
    const WG: (u32, u32) = (16, 16);

    // Consistent shapes that also tile the workgroup exactly validate (device-free).
    #[test]
    fn matmul_shapes_consistent_pass() {
        assert!(validate_matmul_shapes(64, 64, 64, 64 * 64, 64 * 64, 64 * 64, WG).is_ok());
        // Non-square but both m and n are multiples of 16, k arbitrary.
        assert!(validate_matmul_shapes(32, 5, 48, 32 * 5, 5 * 48, 32 * 48, WG).is_ok());
        // A 1D-workgroup contract (used by callers that pass a (16,16)-free size)
        // still tiles when the dims match: here local (7,3) divides (m=6, n=21).
        assert!(validate_matmul_shapes(6, 5, 21, 6 * 5, 5 * 21, 6 * 21, (21, 6)).is_ok());
    }

    // CAN-IT-FAIL: A of the wrong length is rejected with a dimension-named error.
    #[test]
    fn matmul_shape_mismatch_a_is_rejected() {
        // A should be m*k = 15, but we pass 14. (Dims here need not tile the
        // workgroup: the length check runs first.)
        let err = validate_matmul_shapes(3, 5, 7, 14, 35, 21, WG).unwrap_err();
        assert!(
            err.contains('A') && err.contains("m*k"),
            "error should name A and m*k; got: {err}"
        );
    }

    // CAN-IT-FAIL: C of the wrong length is rejected with a dimension-named error.
    #[test]
    fn matmul_shape_mismatch_c_is_rejected() {
        // C should be m*n = 21, but we pass 99.
        let err = validate_matmul_shapes(3, 5, 7, 15, 35, 99, WG).unwrap_err();
        assert!(
            err.contains('C') && err.contains("m*n"),
            "error should name C and m*n; got: {err}"
        );
    }

    // CAN-IT-FAIL: B of the wrong length is rejected with a dimension-named error.
    #[test]
    fn matmul_shape_mismatch_b_is_rejected() {
        let err = validate_matmul_shapes(3, 5, 7, 15, 34, 21, WG).unwrap_err();
        assert!(
            err.contains('B') && err.contains("k*n"),
            "error should name B and k*n; got: {err}"
        );
    }

    // Non-multiple dims are now ACCEPTED: the in-body `if i < m && j < n` guard
    // makes the over-launched invocations no-op, so `validate_matmul_shapes` no
    // longer rejects a grid that does not tile the workgroup exactly. It only
    // checks buffer-length consistency (which holds here). This is the shape the
    // device test `matmul_nonmultiple_dims_match_cpu` exercises on real hardware.
    #[test]
    fn matmul_non_multiple_n_is_accepted() {
        // n = 70 is not a multiple of 16 (70 = 4*16 + 6). Shapes are consistent.
        let (m, k, n) = (64usize, 64usize, 70usize);
        assert!(
            validate_matmul_shapes(m, k, n, m * k, k * n, m * n, WG).is_ok(),
            "non-multiple n must be accepted now that the kernel guards in-body"
        );
    }

    #[test]
    fn matmul_non_multiple_m_is_accepted() {
        // m = 40 is not a multiple of 16 (40 = 2*16 + 8).
        let (m, k, n) = (40usize, 64usize, 64usize);
        assert!(
            validate_matmul_shapes(m, k, n, m * k, k * n, m * n, WG).is_ok(),
            "non-multiple m must be accepted now that the kernel guards in-body"
        );
    }

    // The internal length-consistency check is still load-bearing: an
    // inconsistent buffer length is rejected even for a non-multiple dim.
    #[test]
    fn matmul_inconsistent_length_still_rejected_for_nonmultiple_dims() {
        // n = 33 (non-multiple); C length deliberately wrong.
        let (m, k, n) = (40usize, 40usize, 33usize);
        let err = validate_matmul_shapes(m, k, n, m * k, k * n, m * n + 1, WG).unwrap_err();
        assert!(
            err.contains('C') && err.contains("m*n"),
            "error should name C and m*n; got: {err}"
        );
    }

    // Non-zero workgroup remains required (div_ceil must be well-defined).
    #[test]
    fn matmul_zero_workgroup_is_rejected() {
        let (m, k, n) = (40usize, 40usize, 40usize);
        let err = validate_matmul_shapes(m, k, n, m * k, k * n, m * n, (0, 16)).unwrap_err();
        assert!(
            err.contains("non-zero"),
            "error should name the zero-workgroup violation; got: {err}"
        );
    }
}
