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
//! Scope: the canonical `vec_add`-shaped kernel — N single-dimension f32
//! StorageBuffers at descriptor set 0, bindings 0..N, one invocation per element
//! (`gl_GlobalInvocationID.x`). This is deliberately narrow: it is enough to run
//! the canonical kernel on a real device and cross-check it, not a general Vulkan
//! compute framework.

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

/// Dispatch a compute kernel over `inputs` (each an f32 slice bound as a
/// read-only StorageBuffer at binding i) plus one writable output buffer at the
/// last binding, and return the output readback.
///
/// `spirv` is the compiled module, `entry` the entry-point name, `n` the element
/// count (one invocation per element), `local_size_x` the kernel's declared
/// workgroup X size (used to compute the group count).
pub fn dispatch_vec_add(
    spirv: &[u32],
    entry: &str,
    inputs: &[&[f32]],
    n: usize,
    local_size_x: u32,
) -> HostResult<Vec<f32>> {
    let buffer_count = inputs.len() + 1; // inputs + one output
    unsafe { dispatch_inner(spirv, entry, inputs, n, buffer_count, local_size_x) }
}

unsafe fn dispatch_inner(
    spirv: &[u32],
    entry: &str,
    inputs: &[&[f32]],
    n: usize,
    buffer_count: usize,
    local_size_x: u32,
) -> HostResult<Vec<f32>> {
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
            let byte_len = (n * std::mem::size_of::<f32>()) as vk::DeviceSize;

            // --- Allocate buffers + memory ----------------------------------
            let mut buffers = Vec::with_capacity(buffer_count);
            let mut memories = Vec::with_capacity(buffer_count);
            for _ in 0..buffer_count {
                let (buf, mem) = create_host_visible_buffer(&device, &mem_props, byte_len)?;
                buffers.push(buf);
                memories.push(mem);
            }

            // Upload inputs (bindings 0..inputs.len()).
            for (i, input) in inputs.iter().enumerate() {
                write_buffer_f32(&device, memories[i], input)?;
            }
            // Zero the output buffer (last binding).
            let zeros = vec![0.0f32; n];
            write_buffer_f32(&device, memories[buffer_count - 1], &zeros)?;

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
            let set_layouts = [dsl];
            let pl_ci = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
            let pipeline_layout = device
                .create_pipeline_layout(&pl_ci, None)
                .map_err(|e| format!("create pipeline layout: {e}"))?;

            let sm_ci = vk::ShaderModuleCreateInfo::default().code(spirv);
            let shader = device
                .create_shader_module(&sm_ci, None)
                .map_err(|e| format!("create shader module: {e}"))?;

            let entry_c = std::ffi::CString::new(entry).unwrap();
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
            let group_count = (n as u32).div_ceil(local_size_x.max(1));
            device.cmd_dispatch(cmd, group_count, 1, 1);
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

            // --- Readback ---------------------------------------------------
            let out = read_buffer_f32(&device, memories[buffer_count - 1], n)?;

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
