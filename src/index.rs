use std::ptr::NonNull;
use rustix::{io::Errno, mm};

pub(crate) enum Violation {
    Read,
    Guard
}

pub(crate) enum Action {
    AllocatePage,
    LogAndAbort(Violation)
}

pub enum Zone {
    ReadOnly,
    Lazy,
    Guard
}

// For response of `get_zone_metadata` method
pub struct ZoneMetadata {
    pub zone: Zone,
    pub ptr: NonNull<u8>,
    pub size: usize,
}

// Constants
// These sum up to `NOTEBOOK_SIZE` perfectly.
pub const READ_ONLY_ZONE_SIZE: usize = 256 * 1024 * 1024; // 256 MB
pub const LAZY_ZONE_SIZE: usize = 640 * 1024 * 1024; // 640 MB
pub const GUARD_ZONE_SIZE: usize = 128 * 1024 * 1024; // 128 MB

// Behaves like the index page of the notebook.
pub(crate) struct Index {
    // Read-Only Zone base ptr and size
    rozone_ptr: NonNull<u8>,
    rozone_size: usize,
    // Lazy Zone base ptr and size
    lzone_ptr: NonNull<u8>,
    lzone_size: usize,
    // Gaurd Zone base ptr and size
    gzone_ptr: NonNull<u8>,
    gzone_size: usize
}

impl Index {
    // Partition Notebook into the 3 zones and change protection on READ_ONLY_ZONE
    pub fn new(ptr: NonNull<u8>) -> Result<Self, Errno> {
        // Partioning in ROLG Order.
        let rozone_ptr = ptr;

        // SAFETY: `Notebook` only calls this function with a valid pointer from `mmap`
        // that is guaranteed to be at-least `NOTEBOOK_SIZE`.
        let (lzone_ptr, gzone_ptr) = unsafe {
            let lzone_ptr = rozone_ptr.byte_add(READ_ONLY_ZONE_SIZE);
            let gzone_ptr = lzone_ptr.byte_add(LAZY_ZONE_SIZE);
            (lzone_ptr, gzone_ptr)
        };

        // Adding Read Protection on READ_ONLY_ZONE partition.
        unsafe {
            mm::mprotect(
                rozone_ptr.as_ptr().cast(),
                READ_ONLY_ZONE_SIZE,
                mm::MprotectFlags::READ
            )?
        };

        Ok(Self {
            rozone_ptr,
            rozone_size: READ_ONLY_ZONE_SIZE,

            lzone_ptr,
            lzone_size: LAZY_ZONE_SIZE,

            gzone_ptr,
            gzone_size: GUARD_ZONE_SIZE
        })
    }

    // Return the appropriate action to do if `SIGSEGV` is delivered and
    // the given address was accessed.
    pub fn lookup_action(&self, addr: usize) -> Option<Action> {
        // Convert pointers to usize
        let rozone_addr = self.rozone_ptr.as_ptr() as usize;
        let lzone_addr = self.lzone_ptr.as_ptr() as usize;
        let gzone_addr = self.gzone_ptr.as_ptr() as usize;

        // Check if `addr` falls in READ_ONLY_ZONE
        if addr >= rozone_addr && addr < (rozone_addr + self.rozone_size) {
            return Some(Action::LogAndAbort(Violation::Read));
        }

        // Check if `addr` falls in LAZY_ZONE
        if addr >= lzone_addr && addr < (lzone_addr + self.lzone_size) {
            return Some(Action::AllocatePage);
        }

        // Check if `addr` falls in GUARD_ZONE
        if addr >= gzone_addr && addr < (gzone_addr + self.gzone_size) {
            return Some(Action::LogAndAbort(Violation::Guard));
        }

        // Return None on out of bounds
        None
    }

    // Get pointer and size of the Zone
    pub fn get_zone_metadata(&self, zone: Zone) -> ZoneMetadata {
        match zone {
            Zone::Lazy => ZoneMetadata { zone, ptr: self.lzone_ptr, size: self.lzone_size },
            Zone::ReadOnly => ZoneMetadata { zone, ptr: self.rozone_ptr, size: self.rozone_size },
            Zone::Guard => ZoneMetadata { zone, ptr: self.gzone_ptr, size: self.gzone_size },
        }
    }
}