//! GPU compute subsystem (surface-less wgpu).
//!
//! Today: `compositor` — a per-pixel blend pipeline over `Vec<[f32; 4]>`
//! RGBA buffers that reproduces the CPU compositor's math
//! (`painting::layer::blend_colors`). Degrades gracefully to `Err`/`None`
//! on headless machines so the glow viewport and the rest of the crate
//! never depend on a GPU adapter being present.

mod compositor;

pub use compositor::{
    adapter_name, is_available, shared, Compositor, GpuContext, SUPPORTED_BLEND_MODES,
};
