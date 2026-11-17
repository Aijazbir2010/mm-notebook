use crate::notebook::Notebook;
use crate::index::{Action, Violation};
use crate::telemetry;
use std::ptr::{self, NonNull};
use std::mem::MaybeUninit;
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, Ordering};
use rustix::{io::Errno, mm, param, time};

// ### SETUP & INSTALLATION
// Pointer to the active `Notebook` instance
static ACTIVE_NOTEBOOK: AtomicPtr<Notebook> = AtomicPtr::new(ptr::null_mut());
// This is to ensure that signal handler is only installed once
static INSTALL_ONCE: Once = Once::new();

// To be called by `Notebook::new()` to install the handler and register the
// active `Notebook` instance.
pub(crate) fn register_notebook(notebook: &Notebook) -> Result<(), Errno> {
    // Install handler
    install_sigsegv_handler()?;

    // Update the pointer to point to the active `Notebook` instance.
    let raw_ptr = notebook as *const Notebook as *mut Notebook;
    ACTIVE_NOTEBOOK.store(raw_ptr, Ordering::Release);

    Ok(())
}

// To be called by `Notebook::drop()` to safely clear the pointer before
// unmapping memory.
pub(crate) fn unregister_notebook(notebook: &Notebook) {
    let raw_ptr = notebook as *const Notebook as *mut Notebook;
    let _ = ACTIVE_NOTEBOOK.compare_exchange(
        raw_ptr,
        ptr::null_mut(),
        Ordering::AcqRel,
        Ordering::Relaxed
    );
}

// Allocates on alternate stack and registers the SIGSEGV handler.
fn install_sigsegv_handler() -> Result<(), Errno> {
    let mut res = Ok(());

    // This will be called only once
    INSTALL_ONCE.call_once(|| {
        res = (|| {
            setup_alt_stack()?;
            register_handler()?;
            Ok(())
        })();
    });

    res
}

// Allocates an alternate stack so Stack Overflows don't crash the handler.
fn setup_alt_stack() -> Result<(), Errno> {
    let alt_stack_size: usize = 64 * 1024; // 64 KB

    // Allocate an anonymous private mapping
    let stack_ptr = unsafe {
        let raw = mm::mmap_anonymous(
            ptr::null_mut(),
            alt_stack_size,
            mm::ProtFlags::READ | mm::ProtFlags::WRITE,
            mm::MapFlags::PRIVATE
        )?;

        // SAFETY: if `mmap` succeeds, `raw` will always be a non-null because we have
        // not used `MapFlags::FIXED` in the argument in addition to `ptr::null_mut()`.
        NonNull::new_unchecked(raw)
    };

    // Construct `libc::stack_t` to pass into `libc::sigaltstack`
    let alt_stack = libc::stack_t {
        ss_sp: stack_ptr.as_ptr(),
        ss_flags: 0,
        ss_size: alt_stack_size
    };

    // This manages an alternate stack that can be used for the signal handler execution.
    let res = unsafe {
        libc::sigaltstack(&alt_stack, ptr::null_mut())
    };

    if res == -1 {
        let raw_err = unsafe { *libc::__errno_location() };
        let err = Errno::from_raw_os_error(raw_err);
        return Err(err);
    }

    Ok(())
}

// Registers our custom signal handler
fn register_handler() -> Result<(), Errno> {
    // Create a new zeroed `libc::sigaction`
    let mut action: libc::sigaction = unsafe {
        MaybeUninit::zeroed().assume_init()
    };

    // Address of our custom handler function
    action.sa_sigaction = segv_handler as *const () as usize;
    // SA_SIGINFO passes `siginfo_t` and `ucontext_t` to our handler.
    // SA_ONSTACK runs the handler on our alternate stack.
    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;

    unsafe {
        // Initializes the signal mask (`sa_mask`) to an empty set so that no additional signals are
        // blocked while our custom handler is running.
        libc::sigemptyset(&mut action.sa_mask);

        // Registers our `sigaction` struct (handler) with the OS to intercept SIGSEGV
        if libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut()) == -1 {
            let raw_err = *libc::__errno_location();
            let err = Errno::from_raw_os_error(raw_err);
            return Err(err);
        }
    }

    Ok(())
}

// ### CUSTOM SIGSEGV HANDLER
// This is invoked directly by the Linux Kernel on SIGSEGV
unsafe extern "C" fn segv_handler(
    _signal: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void
) {
    // Starts the timer and automatically records duration on a normal return.
    let _timer = FaultTimer::new();
    // Increments the handler invocation count.
    telemetry::increment_sighandler_count();

    // Extract fault address
    if info.is_null() {
        // Terminate the process immediately if fault address can't be extracted.
        unsafe { libc::_exit(1) }
    }
    let fault_addr = unsafe { (*info).si_addr() as usize };

    // Extract the pointer to `Notebook`` and terminate if null.
    let nb_ptr = ACTIVE_NOTEBOOK.load(Ordering::Acquire);
    if nb_ptr.is_null() {
        unsafe { libc::_exit(1) }
    }

    // Lookup action to do from Index of Notebook
    let notebook = unsafe { &*nb_ptr };
    match notebook.lookup_action(fault_addr) {
        // Lazy Zone Fault
        Some(Action::AllocatePage) => {
            // Align `fault_addr` to the previous page boundary (AA = VA & ~(PSize - 1)).
            let page_size = param::page_size();
            let page_start = fault_addr & !(page_size - 1);

            // Change protection on the page to PROT_READ | PROT_WRITE
            //
            // NOTE: Calling `mprotect` per page splits kernel VMAs (`vm_area_struct`) on
            // non-contiguous writes, which can hit Linux's `vm.max_map_count` limit under
            // heavy sparse access across the Lazy Zone.
            //
            // In a production allocator, we would change protection on larger chunks
            // (e.g. 64 KB-256 KB) or use `userfaultd` to avoid VMA fragmentation. We intentionally
            // keep 1 page change here so every individual page fault is captured by the tracker.
            unsafe {
                if mm::mprotect(
                    page_start as *mut libc::c_void,
                    page_size,
                    mm::MprotectFlags::READ | mm::MprotectFlags::WRITE
                ).is_err() {
                    // Terminate the process if permissions can't be changed.
                    libc::_exit(1)
                }
            };
        }

        // Read-Only Zone or Guard Zone Fault
        Some(Action::LogAndAbort(v)) => {
            // Extract Instruction Pointer (RIP)
            let mut rip = None;
            if !context.is_null() {
                let ucontext = unsafe { &*(context as *const libc::ucontext_t) };
                let extracted_rip = ucontext.uc_mcontext.gregs[libc::REG_RIP as usize];
                rip = Some(extracted_rip as usize);
            }

            // Print report
            match v {
                Violation::Read => {
                    // Print Read Violation Report
                    telemetry::print_security_report(fault_addr, Violation::Read, rip);
                }
                Violation::Guard => {
                    // Print Guard Violation Report
                    telemetry::print_security_report(fault_addr, Violation::Guard, rip);
                }
            }

            // Terminate the process
            unsafe { libc::_exit(1) }
        }

        // Out of Notebook
        None => {
            // Immediately terminate the process if the `fault_addr` is out of
            // our custom allocation.
            unsafe { libc::_exit(1) }
        }
    }
}

// ### HELPERS
// Helper Timer Struct
struct FaultTimer {
    start_ns: u64
}

impl FaultTimer {
    // Start the timer
    #[inline(always)]
    fn new() -> Self {
        Self { start_ns: Self::now_ns() }
    }

    // Get the current time in nanoseconds
    #[inline(always)]
    fn now_ns() -> u64 {
        let now = time::clock_gettime(time::ClockId::Monotonic);
        (now.tv_sec as u64) * 1_000_000_000 + (now.tv_nsec as u64)
    }
}

// Record the time spent in handler when dropped.
impl Drop for FaultTimer {
    #[inline(always)]
    fn drop(&mut self) {
        telemetry::record_sighandler_call(self.start_ns, Self::now_ns());
    }
}