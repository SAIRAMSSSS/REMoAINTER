//! wgpu compute compositor for layer blending.
//!
//! Mirrors the CPU engine's buffer conventions exactly (row-major RGBA
//! `f32` in 0..1, `[w * h]` length — see `painting::graph::ImageRgba`) so
//! results can be swapped in transparently. The blend math reproduces
//! `painting::layer::blend_colors` (branch structure, clamps and guards
//! included), verified by an in-file parity test for every supported mode.
//!
//! Design notes:
//! - `GpuContext::try_new()` fails gracefully with `Err` when no adapter is
//!   available (headless CI, no `/dev/dri`, no lavapipe ICD): callers and
//!   tests must treat the GPU path as an *acceleration*, never a requirement.
//! - `Compositor::blend()` uploads both inputs once, keeps every intermediate
//!   on-device (one dispatch, zero per-pixel round trips) and reads the
//!   result back once.
//! - Modes that only make sense on other pipelines (HSL modes, RNM/Whiteout
//!   normal-map recombination, PassThrough folder semantics) return `Err`,
//!   which lets the caller fall back to the CPU compositor.
//!
//! The existing glow viewport (`graphics::viewport_gl`) is untouched: this
//! module runs entirely on its own surface-less wgpu instance.

use crate::painting::layer::BlendMode;
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use std::sync::{Arc, Mutex, OnceLock};
use wgpu::{Device, Maintain, MapMode, Queue, ShaderSource};

/// Compiled WGSL compute source (`graphics/gpu/shaders/blend.wgsl`).
const BLEND_SHADER_SRC: &str = include_str!("shaders/blend.wgsl");

/// Workgroup tile used by `blend.wgsl` — must match `@workgroup_size(8, 8, 1)`.
const WORKGROUP_X: u32 = 8;
const WORKGROUP_Y: u32 = 8;

/// Blend-mode codes passed to the shader. MUST match the `MODE_*` constants
/// in `shaders/blend.wgsl`.
const MODE_NORMAL: u32 = 0;
const MODE_MULTIPLY: u32 = 1;
const MODE_SCREEN: u32 = 2;
const MODE_OVERLAY: u32 = 3;
const MODE_ADD: u32 = 4;
const MODE_SUBTRACT: u32 = 5;
const MODE_DARKEN: u32 = 6;
const MODE_LIGHTEN: u32 = 7;
const MODE_COLOR_DODGE: u32 = 8;
const MODE_COLOR_BURN: u32 = 9;
const MODE_LINEAR_BURN: u32 = 10;
const MODE_HARD_LIGHT: u32 = 11;
const MODE_SOFT_LIGHT: u32 = 12;
const MODE_DIFFERENCE: u32 = 13;
const MODE_EXCLUSION: u32 = 14;
const MODE_VIVID_LIGHT: u32 = 15;
const MODE_LINEAR_LIGHT: u32 = 16;
const MODE_PIN_LIGHT: u32 = 17;
const MODE_HARD_MIX: u32 = 18;
const MODE_DIVIDE: u32 = 19;
const MODE_DARKER_COLOR: u32 = 20;
const MODE_LIGHTER_COLOR: u32 = 21;

/// Every blend mode the GPU compositor implements today. HSL modes
/// (Color/Luminosity/Hue/Saturation) and the normal-map modes (RNM/Whiteout)
/// remain CPU-only for now — see the module docs.
pub const SUPPORTED_BLEND_MODES: &[BlendMode] = &[
    BlendMode::Normal,
    BlendMode::PassThrough,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Add,
    BlendMode::Subtract,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::LinearBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::VividLight,
    BlendMode::LinearLight,
    BlendMode::PinLight,
    BlendMode::HardMix,
    BlendMode::Divide,
    BlendMode::DarkerColor,
    BlendMode::LighterColor,
];

/// Maps a stack [`BlendMode`] onto the shader's mode code, or `None` when the
/// mode is still CPU-only.
fn mode_code(mode: BlendMode) -> Option<u32> {
    Some(match mode {
        BlendMode::Normal | BlendMode::PassThrough => MODE_NORMAL,
        BlendMode::Multiply => MODE_MULTIPLY,
        BlendMode::Screen => MODE_SCREEN,
        BlendMode::Overlay => MODE_OVERLAY,
        BlendMode::Add => MODE_ADD,
        BlendMode::Subtract => MODE_SUBTRACT,
        BlendMode::Darken => MODE_DARKEN,
        BlendMode::Lighten => MODE_LIGHTEN,
        BlendMode::ColorDodge => MODE_COLOR_DODGE,
        BlendMode::ColorBurn => MODE_COLOR_BURN,
        BlendMode::LinearBurn => MODE_LINEAR_BURN,
        BlendMode::HardLight => MODE_HARD_LIGHT,
        BlendMode::SoftLight => MODE_SOFT_LIGHT,
        BlendMode::Difference => MODE_DIFFERENCE,
        BlendMode::Exclusion => MODE_EXCLUSION,
        BlendMode::VividLight => MODE_VIVID_LIGHT,
        BlendMode::LinearLight => MODE_LINEAR_LIGHT,
        BlendMode::PinLight => MODE_PIN_LIGHT,
        BlendMode::HardMix => MODE_HARD_MIX,
        BlendMode::Divide => MODE_DIVIDE,
        BlendMode::DarkerColor => MODE_DARKER_COLOR,
        BlendMode::LighterColor => MODE_LIGHTER_COLOR,
        // CPU-only for now: HSL modes + normal-map recombinant modes.
        _ => return None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Minimal block-on executor. `pollster` is deliberately not added as a direct
// dependency (only `wgpu` may be added for this module); wgpu's
// request_adapter/request_device futures carry their results in an internal
// oneshot channel that is already filled when the future is returned, so a
// cooperative re-poll loop with a no-op waker is sufficient here.
// ─────────────────────────────────────────────────────────────────────────────

fn block_on<F: std::future::Future>(fut: F) -> Option<F::Output> {
    use std::pin::pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn clone_waker(data: *const ()) -> RawWaker {
        RawWaker::new(data, &VTABLE)
    }
    fn noop_op(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, noop_op, noop_op, noop_op);
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);

    let mut fut = pin!(fut);
    // Bounded spin: these futures are ready on the first meaningful poll in
    // wgpu 22; the counter just guarantees we can't spin forever on a broken
    // backend.
    let spins = AtomicU32::new(0);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return Some(v),
            Poll::Pending => {
                if spins.fetch_add(1, Ordering::Relaxed) > 100_000 {
                    return None;
                }
                std::thread::yield_now();
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GpuContext
// ─────────────────────────────────────────────────────────────────────────────

/// Owned wgpu adapter + device + queue. Cheap to share via `Arc`.
pub struct GpuContext {
    /// Human-readable adapter identity (backend included) for UI/status bars.
    pub adapter_name: String,
    device: Device,
    queue: Queue,
    /// Uncaptured validation/timeout errors, drained by the compositor so
    /// failures surface as `Err` instead of panicking the app.
    errors: Arc<Mutex<Vec<String>>>,
}

impl GpuContext {
    /// Attempts to acquire the best available device. Fails with a
    /// diagnostic-heavy `Err` on headless machines (no adapter, no driver).
    pub fn try_new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .flatten()
        {
            Some(found) => found,
            None => {
                return Err(format!(
                    "no suitable GPU adapter found ({})",
                    availability_hint()
                ))
            }
        };

        let info = adapter.get_info();
        let adapter_name = format!("{} [{:#?}]", info.name, info.backend);

        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let (device, queue) = match block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("gpu-compositor-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )) {
            Some(Ok(pair)) => pair,
            Some(Err(e)) => {
                return Err(format!(
                    "failed to acquire GPU device ({adapter_name}): {e}"
                ))
            }
            None => return Err("GPU device request timed out".to_string()),
        };

        // Collect (rather than panic on) uncaptured device errors; drained by
        // `Compositor::blend` after each submission.
        let handler_errors = errors.clone();
        device.on_uncaptured_error(Box::new(move |error| {
            if let Ok(mut slot) = handler_errors.lock() {
                slot.push(error.to_string());
            }
        }));

        Ok(Self {
            adapter_name,
            device,
            queue,
            errors,
        })
    }

    fn drain_errors(&self) -> Vec<String> {
        match self.errors.lock() {
            Ok(mut slot) => std::mem::take(&mut *slot),
            Err(_) => Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compositor
// ─────────────────────────────────────────────────────────────────────────────

/// Uniform payload pushed to the shader per dispatch (16 bytes, u32-aligned).
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct BlendParams {
    width: u32,
    height: u32,
    mode: u32,
    _reserved: u32,
}

/// A ready-to-dispatch per-pixel blend pipeline.
pub struct Compositor {
    ctx: Arc<GpuContext>,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

impl Compositor {
    /// Creates a compositor on a fresh adapter/device. Returns `Err` on
    /// headless machines so callers can fall back to the CPU path.
    pub fn try_new() -> Result<Self, String> {
        let ctx = Arc::new(GpuContext::try_new()?);
        Ok(Self::from_context(ctx))
    }

    /// Builds the pipelines on a pre-existing context (lets the app share one
    /// device across `is_available()` / `adapter_name()` / real blending).
    pub fn from_context(ctx: Arc<GpuContext>) -> Self {
        let module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("gpu-compositor-shader"),
                source: ShaderSource::Wgsl(Cow::Borrowed(BLEND_SHADER_SRC)),
            });

        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("gpu-compositor-bgl"),
                    entries: &[
                        storage_entry(0, true),
                        storage_entry(1, true),
                        storage_entry(2, false),
                        uniform_entry(3),
                    ],
                });

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gpu-compositor-layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = ctx
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("gpu-compositor-pipeline"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: "main",
                compilation_options: Default::default(),
                cache: None,
            });

        Self {
            ctx,
            bind_group_layout,
            pipeline,
        }
    }

    /// Adapter identity of the underlying context.
    pub fn adapter_name(&self) -> &str {
        &self.ctx.adapter_name
    }

    /// Per-pixel blend of the bottom layer (`a`, blend base) with the top
    /// layer (`b`) in `mode`. Output RGB mirrors `blend_colors(base, top)`
    /// exactly; output alpha carries `b`'s alpha so the stack can still apply
    /// the opacity/mask lerp on the CPU.
    ///
    /// Both inputs are uploaded once, blended with a single on-device
    /// dispatch, and the result is read back once.
    pub fn blend(
        &self,
        a: &[[f32; 4]],
        b: &[[f32; 4]],
        w: u32,
        h: u32,
        mode: BlendMode,
    ) -> Result<Vec<[f32; 4]>, String> {
        let code = mode_code(mode)
            .ok_or_else(|| format!("blend mode {mode:?} is not implemented on GPU yet"))?;
        let pixel_count = (w as usize)
            .checked_mul(h as usize)
            .ok_or_else(|| format!("size {w}x{h} overflows usize"))?;
        if pixel_count == 0 {
            return Err(format!("zero-size blend requested ({w}x{h})"));
        }
        if a.len() < pixel_count {
            return Err(format!(
                "base buffer too short: {} elements for {w}x{h} (need {pixel_count})",
                a.len()
            ));
        }
        if b.len() < pixel_count {
            return Err(format!(
                "top buffer too short: {} elements for {w}x{h} (need {pixel_count})",
                b.len()
            ));
        }

        if let Some(err) = self.ctx.drain_errors().pop() {
            return Err(format!("GPU device error: {err}"));
        }

        let device = &self.ctx.device;
        let base_bytes: &[u8] = bytemuck::cast_slice(&a[..pixel_count]);
        let top_bytes: &[u8] = bytemuck::cast_slice(&b[..pixel_count]);
        let byte_len = base_bytes.len() as u64;

        let params = BlendParams {
            width: w,
            height: h,
            mode: code,
            _reserved: 0,
        };

        // Inputs live in on-device storage buffers (direct write; one upload
        // through the queue's internal staging, no host-side mapping needed).
        let base_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-compositor-base"),
            size: byte_len,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.ctx.queue.write_buffer(&base_buf, 0, base_bytes);

        let top_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-compositor-top"),
            size: byte_len,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.ctx.queue.write_buffer(&top_buf, 0, top_bytes);

        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-compositor-params"),
            size: std::mem::size_of::<BlendParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.ctx
            .queue
            .write_buffer(&params_buf, 0, bytemuck::bytes_of(&params));

        let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-compositor-out"),
            size: byte_len,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-compositor-readback"),
            size: byte_len,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu-compositor-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(base_buf.as_entire_buffer_binding()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(top_buf.as_entire_buffer_binding()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(out_buf.as_entire_buffer_binding()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(params_buf.as_entire_buffer_binding()),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-compositor-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gpu-compositor-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // Padding guard lives in the shader; dispatch covers every pixel
            // even when w/h are not multiples of the workgroup tile.
            let groups_x = w.div_ceil(WORKGROUP_X);
            let groups_y = h.div_ceil(WORKGROUP_Y);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }
        encoder.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, byte_len);
        self.ctx.queue.submit(Some(encoder.finish()));
        device.poll(Maintain::Wait);

        // Read back (one download).
        readback.slice(..).map_async(MapMode::Read, |_| {});
        device.poll(Maintain::Wait);

        if let Some(err) = self.ctx.drain_errors().pop() {
            return Err(format!("GPU blend failed: {err}"));
        }

        let view = readback.slice(..).get_mapped_range();
        let result: Vec<[f32; 4]> = bytemuck::cast_slice(&view).to_vec();
        drop(view);
        readback.unmap();

        if result.len() != pixel_count {
            return Err(format!(
                "GPU readback length mismatch: got {} pixels, expected {pixel_count}",
                result.len()
            ));
        }

        Ok(result)
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(16),
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(16),
        },
        count: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// App detection helpers (global, lazily-created shared Compositor)
// ─────────────────────────────────────────────────────────────────────────────

static SHARED: OnceLock<Option<Arc<Compositor>>> = OnceLock::new();

/// Shared compositor singleton: created on first probe and memoized even when
/// unavailable, so a headless machine doesn't re-probe every frame.
fn shared_compositor() -> Option<Arc<Compositor>> {
    SHARED
        .get_or_init(|| Compositor::try_new().ok().map(Arc::new))
        .clone()
}

/// `true` when a GPU device was found and the compositor pipeline is usable.
/// The first call may take a few hundred ms (adapter probe + shader compile).
pub fn is_available() -> bool {
    shared_compositor().is_some()
}

/// Adapter identity for status bars / diagnostics, if a device was acquired.
pub fn adapter_name() -> Option<String> {
    shared_compositor().map(|c| c.adapter_name().to_owned())
}

/// Snapshot handle to the shared compositor for the app layer.
pub fn shared() -> Option<Arc<Compositor>> {
    shared_compositor()
}

/// Human-readable headless diagnostic: looks for the usual Linux GPU device
/// nodes and Vulkan ICDs (swiftshader / lavapipe included) so tests and logs
/// can explain an unavailable adapter.
fn availability_hint() -> String {
    let mut hints = Vec::new();
    if !std::path::Path::new("/dev/dri").exists() {
        hints.push("no /dev/dri device nodes".to_string());
    }
    let icd_count = std::fs::read_dir("/usr/share/vulkan/icd.d/")
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0);
    if icd_count == 0 {
        hints.push("no Vulkan ICDs under /usr/share/vulkan/icd.d".to_string());
    }
    if hints.is_empty() {
        format!(
            "driver nodes present (device nodes + {icd_count} Vulkan ICD) but no adapter accepted ({:?})",
            wgpu::Backends::all()
        )
    } else {
        hints.join("; ")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painting::layer::blend_colors;

    /// Every GPU test goes through this: `None` => the machine is headless and
    /// the test must skip (eprintln, no failure) so CI stays green. The skip
    /// path prints the usual availability predictors (/dev/dri, Vulkan ICDs).
    fn gpu_or_skip() -> Option<Compositor> {
        match Compositor::try_new() {
            Ok(c) => {
                eprintln!("GPU available: {}", c.adapter_name());
                Some(c)
            }
            Err(e) => {
                let dri = std::path::Path::new("/dev/dri").exists();
                let icds = std::fs::read_dir("/usr/share/vulkan/icd.d/")
                    .map(|entries| entries.filter_map(Result::ok).count())
                    .unwrap_or(0);
                eprintln!(
                    "SKIP (no GPU adapter on this machine): {e} -- /dev/dri={dri}, vulkan ICDs={icds}"
                );
                None
            }
        }
    }

    /// Deterministic pseudo-random source (xorshift32).
    struct Rng(u32);
    impl Rng {
        fn next_32bits(&mut self) -> u32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.0 = x;
            x
        }
        fn next_unit(&mut self) -> f32 {
            (self.next_32bits() >> 8) as f32 / 16_777_216.0
        }
    }

    /// Pixel pair with seeded pseudo-random values *plus* deliberate edge
    /// values (0, 0.25, 0.5, 1.0) so every branch in every blend mode fires.
    fn parity_buffers(len: usize) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let mut rng = Rng(0x5EED_A11C);
        let edges = [0.0f32, 0.25, 0.5, 1.0];
        let base: Vec<[f32; 4]> = (0..len)
            .map(|i| {
                let e = if i % 7 == 0 {
                    edges[(i / 7) % edges.len()]
                } else {
                    rng.next_unit()
                };
                [e, rng.next_unit(), rng.next_unit(), rng.next_unit()]
            })
            .collect();
        let top: Vec<[f32; 4]> = (0..len)
            .map(|i| {
                let e = if i % 11 == 0 {
                    edges[(i / 11) % edges.len()]
                } else {
                    rng.next_unit()
                };
                [rng.next_unit(), e, rng.next_unit(), rng.next_unit()]
            })
            .collect();
        (base, top)
    }

    #[test]
    fn parity_gpu_vs_cpu_all_supported_modes_64x64() {
        let Some(comp) = gpu_or_skip() else { return };
        const W: u32 = 64;
        const H: u32 = 64;
        for &mode in SUPPORTED_BLEND_MODES {
            let (base, top) = parity_buffers((W * H) as usize);
            let gpu = comp
                .blend(&base, &top, W, H, mode)
                .unwrap_or_else(|e| panic!("GPU blend {mode:?} failed: {e}"));
            for (i, ((gp, tp), up)) in gpu.iter().zip(&top).zip(&base).enumerate() {
                let expect = blend_colors([up[0], up[1], up[2]], [tp[0], tp[1], tp[2]], mode);
                for c in 0..3 {
                    let diff = (gp[c] - expect[c]).abs();
                    assert!(
                        diff <= 1e-4,
                        "mode {mode:?} pixel {i}: rgb.{c} gpu={} cpu={} (Δ={diff:e})",
                        gp[c],
                        expect[c]
                    );
                }
                assert_eq!(
                    gp[3], tp[3],
                    "mode {mode:?} pixel {i}: alpha must carry the top buffer"
                );
            }
        }
    }

    #[test]
    fn buffers_not_multiple_of_workgroup_70x50() {
        let Some(comp) = gpu_or_skip() else { return };
        // 70 and 50 are both non-multiples of 8 — exercises the padding guard.
        const W: u32 = 70;
        const H: u32 = 50;
        let len = (W * H) as usize;
        let (base, top) = parity_buffers(len);

        for mode in [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::VividLight,
        ] {
            let gpu = comp
                .blend(&base, &top, W, H, mode)
                .unwrap_or_else(|e| panic!("GPU blend {mode:?} failed: {e}"));
            assert_eq!(
                gpu.len(),
                len,
                "output length must be exactly w*h, never workgroup-padded"
            );
            for (i, ((gp, tp), up)) in gpu.iter().zip(&top).zip(&base).enumerate() {
                let expect = blend_colors([up[0], up[1], up[2]], [tp[0], tp[1], tp[2]], mode);
                for c in 0..3 {
                    assert!(
                        (gp[c] - expect[c]).abs() <= 1e-4,
                        "mode {mode:?} pixel {i} (x={} y={}): rgb.{c} gpu={} cpu={}",
                        i % W as usize,
                        i / W as usize,
                        gp[c],
                        expect[c]
                    );
                }
            }
        }
    }

    #[test]
    fn zero_size_and_mismatched_inputs_are_errors() {
        let Some(comp) = gpu_or_skip() else { return };
        let four: Vec<[f32; 4]> = vec![[0.5, 0.5, 0.5, 1.0]; 4];

        assert!(
            comp.blend(&four, &four, 0, 1, BlendMode::Normal).is_err(),
            "w=0 must be Err"
        );
        assert!(
            comp.blend(&four, &four, 1, 0, BlendMode::Normal).is_err(),
            "h=0 must be Err"
        );
        assert!(
            comp.blend(&four, &four, 0, 0, BlendMode::Normal).is_err(),
            "0x0 must be Err"
        );
        // Buffers shorter than the requested area.
        let three: Vec<[f32; 4]> = vec![[0.5, 0.5, 0.5, 1.0]; 3];
        assert!(
            comp.blend(&three, &four, 2, 2, BlendMode::Normal).is_err(),
            "short base buffer must be Err"
        );
        assert!(
            comp.blend(&four, &three, 2, 2, BlendMode::Normal).is_err(),
            "short top buffer must be Err"
        );
        // Exactly 2x2 is fine and yields 4 pixels.
        let ok = comp
            .blend(&four, &four, 2, 2, BlendMode::Normal)
            .expect("2x2 must blend");
        assert_eq!(ok.len(), 4);
        // Larger buffers are allowed: only the first w*h pixels are consumed.
        let big_base: Vec<[f32; 4]> = vec![[0.0, 0.0, 0.0, 1.0]; 10];
        let big_top: Vec<[f32; 4]> = vec![[1.0, 0.5, 0.25, 1.0]; 10];
        let got = comp
            .blend(&big_base, &big_top, 2, 2, BlendMode::Normal)
            .expect("oversized inputs ok");
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], big_top[0], "Normal mode forwards the top pixel");
    }

    #[test]
    fn cpu_only_modes_report_error() {
        let Some(comp) = gpu_or_skip() else { return };
        let a: Vec<[f32; 4]> = vec![[0.5; 4]; 4];
        for mode in [
            BlendMode::Color,
            BlendMode::Luminosity,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::RNM,
            BlendMode::Whiteout,
        ] {
            let err = comp.blend(&a, &a, 2, 2, mode);
            assert!(err.is_err(), "mode {mode:?} should still be CPU-only");
        }
    }

    #[test]
    fn shared_helpers_do_not_panic_headless() {
        // Formatting-only probe of the module-level detection helpers.
        if is_available() {
            assert!(
                adapter_name().is_some(),
                "available => adapter name must be reported"
            );
        } else {
            eprintln!("shared helpers: GPU reports unavailable on this machine");
        }
    }
}
