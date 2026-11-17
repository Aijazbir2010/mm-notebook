use crate::index::{Index, Action, Zone, ZoneMetadata};
use crate::sighandler;
use std::ptr::{self, NonNull};
use rustix::{io::Errno, mm};

// Notebook Size - 1 GB
pub const NOTEBOOK_SIZE: usize = 1024 * 1024 * 1024;

pub struct Notebook {
    nb_ptr: NonNull<u8>, // Base pointer of the memory allocation
    nb_size: usize, // Total size of the memory allocation
    index: Index // Tracks location and sizes of the 3 zones
}

// Implement `Send` and `Sync` for `Notebook`
// SAFETY: The memory allocation is owned exclusively by `Notebook` and is not tied
// to any specific thread. It can be safely transferred and unmapped on another thread.
unsafe impl Send for Notebook {}

// SAFETY: `Notebook` provides no safe methods to mutate the underlying memory.
// Any mutation via the raw pointer requires an `unsafe` block, placing the burder of
// synchronization on the caller.
unsafe impl Sync for Notebook {}

impl Notebook {
    // Construct a new `Notebook` instance.
    pub fn new() -> Result<Box<Self>, Errno> {
        // Call `mmap()` to allocate a private anonymous mapping pool.
        let ptr = unsafe {
            let raw = mm::mmap_anonymous(
                ptr::null_mut(),
                NOTEBOOK_SIZE,
                mm::ProtFlags::empty(), // PROT_NONE
                mm::MapFlags::PRIVATE, // Private Anonymous Mapping
            )?;

            // SAFETY: if `mmap` succeeds, `raw` will always be a non-null because we have
            // not used `MapFlags::FIXED` in the argument in addition to `ptr::null_mut()`.
            NonNull::new_unchecked(raw.cast::<u8>())
        };

        // Index partions Notebook into the 3 zones and changes protection on READ_ONLY_ZONE.
        let index = Index::new(ptr)?;

        // Create a new `Notebook` instance on heap.
        let notebook = Box::new(Self {
            nb_ptr: ptr,
            nb_size: NOTEBOOK_SIZE,
            index,
        });

        // Register the sighandler and initialize the pointer to this `Notebook` instance.
        sighandler::register_notebook(&notebook)?;

        Ok(notebook)
    }

    // Wrapper for `index.lookup_action` method
    pub(crate) fn lookup_action(&self, addr: usize) -> Option<Action> {
        self.index.lookup_action(addr)
    }

    // Wrapper for `index.get_zone_metadata` method
    pub fn get_zone_metadata(&self, zone: Zone) -> ZoneMetadata {
        self.index.get_zone_metadata(zone)
    }

    // Get size of `Notebook`
    pub fn size(&self) -> usize {
        self.nb_size
    }
}

// Unmap the mapped region on drop
impl Drop for Notebook {
    fn drop(&mut self) {
        // Unregister this `Notebook` instance before dropping allocation.
        sighandler::unregister_notebook(self);

        let _ = unsafe {
            // SAFETY: It is safe because we are using the pointer that was returned by
            // `mmap` itself and length is also same that we used at the time of `mmap`.
            mm::munmap(
                self.nb_ptr.as_ptr().cast(),
                self.nb_size
            )
        };
    }
}