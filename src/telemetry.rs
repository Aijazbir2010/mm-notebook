use crate::notebook::NOTEBOOK_SIZE;
use crate::index::Violation;
use std::sync::atomic::{AtomicU64, Ordering};
use rustix::io;

// Total time spent in sighandler in nanoseconds
static TOTAL_SIGHANDLER_NS: AtomicU64 = AtomicU64::new(0);
// Number of times sighandler was invoked
static SIGHANDLER_COUNT: AtomicU64 = AtomicU64::new(0);

// It's async-signal-safe because it only performs lock-free atomic additions.
#[inline(always)]
pub(crate) fn record_sighandler_call(start_ns: u64, end_ns: u64) {
    let elapsed = end_ns.saturating_sub(start_ns);
    TOTAL_SIGHANDLER_NS.fetch_add(elapsed, Ordering::Relaxed);
}

// It's async-signal-safe because it only performs lock-free atomic additions.
#[inline(always)]
pub(crate) fn increment_sighandler_count() {
    SIGHANDLER_COUNT.fetch_add(1, Ordering::Relaxed);
}

// Print audit report after a successful finish
pub fn print_audit_report() {
    let pg_size = rustix::param::page_size();
    let faults = SIGHANDLER_COUNT.load(Ordering::Acquire);
    let time_spent = TOTAL_SIGHANDLER_NS.load(Ordering::Acquire);

    println!();
    println!("=============================================");
    println!(" MMNotebook Execution Telemetry Report");
    println!("=============================================");
    println!(" Notebook Size        : {} bytes ({:.2} GiB)",
        usize_to_formatted_str(NOTEBOOK_SIZE),
        NOTEBOOK_SIZE as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!(" Page Size            : {pg_size} bytes");
    println!(" Total Pages Managed  : {}",
        usize_to_formatted_str(NOTEBOOK_SIZE / pg_size)
    );
    println!("---------------------------------------------");
    println!(" Faults Triggered     : {faults}");
    println!("   - Lazy Allocations : {faults} pages populated on demand");
    println!(" Time in Handler      : {} nanoseconds total",
        usize_to_formatted_str(time_spent as usize)
    );
    println!("=============================================");
    println!();
}

// Helper to convert usize to formatted string
fn usize_to_formatted_str(n: usize) -> String {
    n.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join(",")
}

// Print security audit report before abort in sighandler (Needs to be async-signal-safe)
pub(crate) fn print_security_report(f_addr: usize, v: Violation, rip: Option<usize>) {
    let faults = SIGHANDLER_COUNT.load(Ordering::Acquire);

    write_all_raw(b"\n=============================================\n");
    write_all_raw(b" MMNotebook Security Audit Report\n");
    write_all_raw(b"=============================================\n");
    write_all_raw(b" Faults Triggered     : ");
    write_u64(faults);
    write_all_raw(b"\n");
    write_all_raw(b"---------------------------------------------\n");
    write_all_raw(b" [CRITICAL INCIDENT]\n");
    write_all_raw(b" - Fault Address      : ");
    write_hex(f_addr);
    write_all_raw(b"\n");

    match v {
        Violation::Read => {
            write_all_raw(b" - Fault Type         : Unauthorized write on Read-Only Page\n");
        }
        Violation::Guard => {
            write_all_raw(b" - Fault Type         : Unauthorized access on Guard Page\n");
        }
    }

    write_all_raw(b" - Faulting RIP       : ");
    if let Some(rip) = rip {
        write_hex(rip);
    } else {
        write_all_raw(b"<Unavailable>");
    }
    write_all_raw(b"\n");

    write_all_raw(b" - Action Taken       : Terminated process (SIGSEGV Aborted)\n");
    write_all_raw(b"=============================================\n");
}

// Helper to write a raw byte slice, handling partial writes and EINTR safely
fn write_all_raw(mut buf: &[u8]) {
    // SAFETY: stderr() returns a borrowed raw fd (2) with no locks or allocations
    let fd = rustix::stdio::stderr();
    while !buf.is_empty() {
        match io::write(fd, buf) {
            Ok(0) => break,
            Ok(n) => buf = &buf[n..],
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => break,
        }
    }
}

// Formats a usize as a zero-padded 16-character hex string (e.g., 0x00007fff12345678)
fn write_hex(mut n: usize) {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [b'0'; 18];
    buf[0] = b'0';
    buf[1] = b'x';

    for i in (2..18).rev() {
        buf[i] = HEX_CHARS[n & 0xf];
        n >>= 4;
    }

    write_all_raw(&buf);
}

// Helper to write a u64 as decimal ASCII without heap allocation
fn write_u64(mut n: u64) {
    if n == 0 {
        write_all_raw(b"0");
        return;
    }

    // A u64 has at most 20 decimal digits
    let mut buf = [0u8; 20];
    let mut i = buf.len();

    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    write_all_raw(&buf[i..]);
}