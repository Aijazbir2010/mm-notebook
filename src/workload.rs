use crate::index::LAZY_ZONE_SIZE;
use std::ptr::{self, NonNull};

/// Example workload for testing Lazy Zone.
/// 
/// # Errors
/// 
/// Returns an `Err` if `data_length` is `0` or exceeds `mm_notebook::LAZY_ZONE_SIZE`.
/// 
/// # Safety
/// 
/// - `lzone_ptr` must be valid for writes of `data_length` bytes.
/// - The entire memory range `[lzone_ptr, lzone_ptr + data_length)` must be within
///   a single allocated Notebook Lazy Zone area.
pub unsafe fn lzone_write(lzone_ptr: NonNull<u8>, data_length: usize) -> Result<(), String> {
    if data_length == 0 {
        return Err("Data length must be at-least 1 byte.".to_string());
    } else if data_length > LAZY_ZONE_SIZE {
        return Err(format!("Data length must less than or equal to {} bytes", LAZY_ZONE_SIZE));
    }

    let base = lzone_ptr.as_ptr();
    let page_size = rustix::param::page_size();
    let mut offset = 0;

    while offset < data_length {
        // SAFETY: `data_length` is verified to be within `1..=LAZY_ZONE_SIZE` and
        // the caller guarantees `lzone_ptr` is valid for writes accross that range.
        unsafe {
            ptr::write_volatile(base.add(offset), 0x01);
        }
        offset += page_size;
    }

    // Always touch the final byte so an unaligned range never misses its last page.
    unsafe {
        ptr::write_volatile(base.add(data_length - 1), 0x01);
    }

    Ok(())
}

/// Example workload for testing Read-Only Zone.
/// 
/// Attempting to write to this zone is expected to print a security
/// audit report and terminate the process.
/// 
/// # Safety
/// 
/// - `rozone_ptr` must fall within a Notebook's Read Only Zone area.
pub unsafe fn rozone_write(rozone_ptr: NonNull<u8>) {
    // SAFETY: Caller guarantees `rozone_ptr` points within the Read-Only Zone.
    unsafe { ptr::write_volatile(rozone_ptr.as_ptr(), 0x01); }
}

/// Example workload for testing Guard Zone.
/// 
/// Attempting to write to this zone is expected to print a security
/// audit report and terminate the process.
/// 
/// # Safety
/// 
/// - `gzone_ptr` must fall within a Notebook's Guard Zone area.
pub unsafe fn gzone_write(gzone_ptr: NonNull<u8>) {
    // SAFETY: Caller guarantees `gzone_ptr` points within the Guard Zone.
    unsafe { ptr::write_volatile(gzone_ptr.as_ptr(), 0x01); }
}