//! Hardware ray-tracing capability probe (Vulkan `VK_KHR_ray_query`
//! / `VK_KHR_ray_tracing_pipeline`). The CPU (Rayon) baker is always the
//! fallback path; hardware ray tracing is enabled only when the local Vulkan
//! loader reports a device exposing the required extensions.

use std::sync::OnceLock;

/// Result of probing the local Vulkan implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtCapability {
    /// Required: ray queries usable from compute stages.
    pub ray_query: bool,
    /// Optional: full ray-tracing pipelines (miss/closest-hit stages).
    pub ray_tracing_pipeline: bool,
    /// Number of Vulkan physical devices that responded.
    pub devices: usize,
}

/// Probe hardware ray-tracing support. `Err` describes why no Vulkan loader
/// or ICD is available; the caller keeps the CPU baker active in that case.
pub fn probe() -> Result<RtCapability, String> {
    static PROBED: OnceLock<Result<RtCapability, String>> = OnceLock::new();
    PROBED.get_or_init(probe_uncached).clone()
}

fn probe_uncached() -> Result<RtCapability, String> {
    let miss = || "no Vulkan loader or ray-tracing-capable device detected".to_string();
    let _ = miss;
    let (entry, instance) = unsafe {
        let entry = ash::Entry::load().map_err(|e| format!("no Vulkan loader: {e}"))?;
        let app_name =
            std::ffi::CString::new("remoainter-rt-probe").expect("static cstr name");
        let app_info = ash::vk::ApplicationInfo::default()
            .application_name(&app_name)
            .api_version(ash::vk::make_api_version(0, 1, 3, 0));
        let create_info = ash::vk::InstanceCreateInfo::default().application_info(&app_info);
        let instance = entry
            .create_instance(&create_info, None)
            .map_err(|e| format!("Vulkan instance creation failed: {e}"))?;
        (entry, instance)
    };
    let mut cap = RtCapability {
        ray_query: false,
        ray_tracing_pipeline: false,
        devices: 0,
    };
    unsafe {
        let ph = match instance.enumerate_physical_devices() {
            Ok(v) => v,
            Err(e) => {
                instance.destroy_instance(None);
                return Err(format!("enumerating Vulkan devices failed: {e}"));
            }
        };
        cap.devices = ph.len();
        for phys in &ph {
            let exts = instance
                .enumerate_device_extension_properties(*phys)
                .unwrap_or_default();
            for e in &exts {
                let name = e.extension_name_as_c_str().unwrap_or_default().to_string_lossy();
                classify(name.as_ref(), &mut cap);
            }
        }
    }
    if !cap.ray_query && !cap.ray_tracing_pipeline {
        return Err(format!(
            "no ray-tracing-capable Vulkan device found ({} devices present)",
            cap.devices
        ));
    }
    Ok(cap)
}

/// Convenience: `true` only when ray queries are actually usable.
pub fn ray_query_available() -> bool {
    probe().map(|c| c.ray_query).unwrap_or(false)
}

/// Classify a Vulkan extension name for the probe result.
fn classify(name: &str, cap: &mut RtCapability) {
    match name {
        "VK_KHR_ray_query" => cap.ray_query = true,
        "VK_KHR_ray_tracing_pipeline" => cap.ray_tracing_pipeline = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognizes_rt_extensions() {
        let mut cap = RtCapability { ray_query: false, ray_tracing_pipeline: false, devices: 0 };
        classify("VK_KHR_ray_query", &mut cap);
        classify("VK_KHR_surface", &mut cap);
        assert!(cap.ray_query && !cap.ray_tracing_pipeline);
    }

    #[test]
    fn instance_probe_only_opt_in() {
        // Creating a VkInstance races with glow/wayland tests on some stacks,
        // so the real probe is wired at app startup (pre-UI, single-threaded).
        // Tests can opt in explicitly:
        if std::env::var_os("REMOAINTER_RT_PROBE_TEST").is_some() {
            let _ = probe();
        } else {
            std::env::set_var("REMOAINTER_RT_PROBE_TEST", "0");
        }
    }
}
