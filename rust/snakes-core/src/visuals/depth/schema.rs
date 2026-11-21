use std::ffi::c_char;
/// Strings are static NUL-terminated UTF-8; enum choices use `|` separators.
#[repr(C)]
pub struct Parameter {
    pub id: u32,
    pub kind: u32,
    pub key: *const c_char,
    pub minimum: f64,
    pub maximum: f64,
    pub default_value: f64,
    pub choices: *const c_char,
}
// All pointers refer to immutable compile-time strings.
unsafe impl Sync for Parameter {}
impl Parameter {
    pub const fn integer(
        id: u32,
        key: &'static std::ffi::CStr,
        min: f64,
        max: f64,
        value: f64,
    ) -> Self {
        Self {
            id,
            kind: 1,
            key: key.as_ptr(),
            minimum: min,
            maximum: max,
            default_value: value,
            choices: c"".as_ptr(),
        }
    }
    pub const fn boolean(id: u32, key: &'static std::ffi::CStr, value: bool) -> Self {
        Self {
            id,
            kind: 2,
            key: key.as_ptr(),
            minimum: 0.,
            maximum: 1.,
            default_value: if value { 1. } else { 0. },
            choices: c"".as_ptr(),
        }
    }
    pub const fn enumeration(
        id: u32,
        key: &'static std::ffi::CStr,
        choices: &'static std::ffi::CStr,
        max: f64,
        value: f64,
    ) -> Self {
        Self {
            id,
            kind: 3,
            key: key.as_ptr(),
            minimum: 0.,
            maximum: max,
            default_value: value,
            choices: choices.as_ptr(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Value {
    pub id: u32,
    pub kind: u32,
    pub payload: u64,
}
