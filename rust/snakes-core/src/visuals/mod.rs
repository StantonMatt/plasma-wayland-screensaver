// SPDX-License-Identifier: GPL-3.0-or-later
//! Versioned procedural visual ABI. No Snakes state or symbols are changed.
//! Callers own output memory; handles retain no borrowed pointers. A handle is
//! single-threaded; the GUI builds snapshots, render threads own immutable copies.
pub mod aurora;
pub mod palette;
pub mod phase;
pub mod primitives;
pub mod random;
pub mod settings;
pub mod strokes;
use settings::Config;
pub const ABI_VERSION: u32 = 2;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Band {
    pub geometry: [f32; 4],
    pub inner: [f32; 4],
    pub middle: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Frame {
    pub bands: [Band; 9],
    pub shade: [f32; 4],
    pub viewport: [f32; 2],
    pub band_count: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Request {
    pub width: f64,
    pub height: f64,
    pub presentation_ns: i64,
    /// 0 keeps the previous phase (including initial reduced-motion state).
    pub advance: u32,
    pub reserved: u32,
    /// Global world rect and viewport origin. Aurora deliberately uses local
    /// width/height in all monitor modes, preserving the QML mapping.
    pub world_bounds: [f64; 4],
    pub viewport_origin: [f64; 2],
}
pub struct Core {
    config: Config,
    phase: f64,
    seeds: primitives::Seeds,
}

#[unsafe(no_mangle)]
pub extern "C" fn visuals_core_abi_version() -> u32 {
    ABI_VERSION
}
/// # Safety
/// `config` must point to a readable v2 Config record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_create(config: *const Config) -> *mut Core {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    if !config.valid() {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(Core {
        config: *config,
        phase: initial_phase(config),
        seeds: primitives::Seeds::new(config.seed),
    }))
}
/// # Safety
/// The handle must be live/exclusively borrowed; config readable. Returns 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_configure(core: *mut Core, config: *const Config) -> i32 {
    let (Some(core), Some(config)) = (unsafe { core.as_mut() }, unsafe { config.as_ref() }) else {
        return -1;
    };
    if !config.valid() {
        return -1;
    }
    if core.config.seed != config.seed {
        core.seeds = primitives::Seeds::new(config.seed);
    }
    if core.config.visual != config.visual
        || (config.visual == 1 && core.config.seed != config.seed)
    {
        core.phase = initial_phase(config);
    }
    core.config = *config;
    0
}
/// # Safety
/// Destroy a live handle exactly once. Null is accepted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_destroy(core: *mut Core) {
    if !core.is_null() {
        drop(unsafe { Box::from_raw(core) });
    }
}
/// # Safety
/// Handle exclusively borrowed; request readable; out points to out_capacity
/// writable bytes aligned for Frame. Failure leaves output untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_frame(
    core: *mut Core,
    request: *const Request,
    out: *mut Frame,
    out_capacity: usize,
) -> i32 {
    let (Some(core), Some(request)) = (unsafe { core.as_mut() }, unsafe { request.as_ref() })
    else {
        return -1;
    };
    if out.is_null()
        || out_capacity < size_of::<Frame>()
        || core.config.visual != 1
        || !request.valid()
    {
        return -1;
    }
    core.advance(request);
    unsafe { out.write(aurora::frame(&core.config, core.phase, request)) };
    0
}
/// # Safety
/// frame readable, pixels writable for capacity bytes, without overlap. RGBA8 premultiplied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_raster(
    frame: *const Frame,
    width: u32,
    height: u32,
    pixels: *mut u8,
    capacity: usize,
) -> i32 {
    let Some(frame) = (unsafe { frame.as_ref() }) else {
        return -1;
    };
    let Some(bytes) = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return -1;
    };
    if pixels.is_null()
        || bytes > capacity
        || width == 0
        || height == 0
        || frame.band_count > 9
        || frame.viewport.iter().any(|v| !v.is_finite() || *v <= 0.0)
    {
        return -1;
    }
    aurora::raster(
        frame,
        unsafe { std::slice::from_raw_parts_mut(pixels, bytes) },
        width as usize,
        height as usize,
    );
    0
}
fn initial_phase(config: &Config) -> f64 {
    if config.visual == 1 {
        phase::initial(config.seed)
    } else {
        0.0
    }
}
impl Request {
    fn valid(&self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && self.width <= 65536.0
            && self.height <= 65536.0
            && self.advance <= 1
            && self.reserved == 0
            && self
                .world_bounds
                .iter()
                .chain(self.viewport_origin.iter())
                .all(|v| v.is_finite())
            && self.world_bounds[2] >= 0.0
            && self.world_bounds[3] >= 0.0
    }
}
impl Core {
    fn advance(&mut self, request: &Request) {
        if request.advance != 0 && self.config.reduced_motion == 0 {
            let elapsed =
                (request.presentation_ns as i128 - self.config.epoch_ns as i128) as f64 * 1e-9;
            self.phase = match self.config.visual {
                1 => phase::aurora(
                    self.config.seed,
                    self.config.speed,
                    self.config.epoch_ns,
                    request.presentation_ns,
                ),
                2 => elapsed * 0.35,
                _ => elapsed * 0.12,
            };
        }
    }
}
/// # Safety
/// Exclusive live handle, readable request; output is caller-owned aligned records.
/// Capacity is records (not bytes). Count and output are untouched on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_primitives(
    core: *mut Core,
    request: *const Request,
    out: *mut primitives::Primitive,
    capacity: usize,
    count: *mut u32,
) -> i32 {
    let (Some(core), Some(request)) = (unsafe { core.as_mut() }, unsafe { request.as_ref() })
    else {
        return -1;
    };
    if core.config.visual == 1
        || !request.valid()
        || out.is_null()
        || count.is_null()
        || capacity < primitives::capacity(core.config.visual)
    {
        return -1;
    }
    core.advance(request);
    let records = unsafe { std::slice::from_raw_parts_mut(out, capacity) };
    let n = primitives::frame(&core.config, &core.seeds, core.phase, request, records);
    unsafe { count.write(n as u32) };
    0
}
const _: () = {
    assert!(size_of::<Config>() == 64);
    assert!(std::mem::offset_of!(Config, epoch_ns) == 56);
    assert!(size_of::<Band>() == 48);
    assert!(std::mem::offset_of!(Band, inner) == 16);
    assert!(std::mem::offset_of!(Band, middle) == 32);
    assert!(size_of::<Frame>() == 464);
    assert!(std::mem::offset_of!(Frame, shade) == 432);
    assert!(size_of::<Request>() == 80);
    assert!(std::mem::offset_of!(Request, world_bounds) == 32);
};
#[cfg(test)]
mod tests;
/// # Safety
/// Input/output buffers valid for count records and count*6 vertices. No overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_primitive_vertices(
    records: *const primitives::Primitive,
    count: usize,
    fringe: f32,
    out: *mut primitives::Vertex,
    capacity: usize,
) -> i32 {
    if records.is_null()
        || out.is_null()
        || count > primitives::MAX_RECORDS
        || capacity < count * 6
        || !fringe.is_finite()
        || fringe <= 0.0
    {
        return -1;
    }
    primitives::vertices(
        unsafe { std::slice::from_raw_parts(records, count) },
        fringe,
        unsafe { std::slice::from_raw_parts_mut(out, count * 6) },
    );
    0
}
/// # Safety
/// Readable records; pixels writable for capacity bytes, no overlap. RGBA8 premultiplied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_core_primitive_raster(
    records: *const primitives::Primitive,
    count: usize,
    viewport_width: f32,
    viewport_height: f32,
    width: u32,
    height: u32,
    pixels: *mut u8,
    capacity: usize,
) -> i32 {
    let Some(bytes) = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return -1;
    };
    if records.is_null()
        || pixels.is_null()
        || count > primitives::MAX_RECORDS
        || bytes > capacity
        || width == 0
        || height == 0
        || !viewport_width.is_finite()
        || !viewport_height.is_finite()
        || viewport_width <= 0.0
        || viewport_height <= 0.0
    {
        return -1;
    }
    primitives::raster(
        unsafe { std::slice::from_raw_parts(records, count) },
        [viewport_width, viewport_height],
        unsafe { std::slice::from_raw_parts_mut(pixels, bytes) },
        width as usize,
        height as usize,
    );
    0
}
