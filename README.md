# mm-notebook (`mm-notebook` / `mmnb`)

> A fault-driven virtual memory sentinel, sandbox, and telemetry engine built from first principles in Rust.

`mm-notebook` (`mm_notebook`) is a low-level systems library and command-line utility (`mmnb`) designed to explore and manipulate virtual memory management primitives on Linux. It carves out a controlled 1 GB anonymous virtual memory arena, partitions it into distinct functional zones, intercepts hardware memory exceptions (`SIGSEGV`) via custom signal handlers, and enforces fine-grained access policies on the fly.

---

## 🏗️ Architectural Overview

`mm-notebook` treats a 1 GB virtual memory region like a structured notebook divided into three specialized zones:

1. **Read-Only Zone (256 MB):** Protected with `PROT_READ`. Any unauthorized write attempt triggers a hardware MMU fault, which is caught and logged as a security violation.
2. **Lazy-Allocation Zone (640 MB):** Initially unbacked by physical frames. Accessing it triggers a page fault (`SIGSEGV`), allowing the engine to allocate and map physical memory dynamically on demand using `mprotect`.
3. **Guard Zone (128 MB):** Protected with `PROT_NONE` to act as an absolute boundary trap (simulating stack overflow protection). Any access immediately halts execution and generates a forensic incident report.

---

## ⚙️ Key Technical Highlights

* **True Async-Signal-Safety:** Signal handlers bypass standard library formatting and heap allocation entirely, implementing custom stackless decimal/hex formatters (`write_hex`, `write_u64`) writing directly to `stderr` (FD 2) to prevent deadlocks.
* **Dedicated Alternate Stack (`sigaltstack`):** Registers an isolated 64 KB stack (`SA_ONSTACK`) for the signal handler, ensuring that guard page overflows never trigger double-faults or stack corruption.
* **RAII Execution Timers:** Uses Drop guards with monotonic clocks (`clock_gettime`) to precisely measure nanoseconds spent inside signal handling routines.
* **Forensic CPU Context Inspection:** Extracts instruction pointers (`RIP`) from underlying kernel `ucontext_t` structures during critical exceptions to pinpoint exact fault origins.
* **Zero-Cost Abstractions:** Built using low-level system bindings (`rustix` and `libc`) with strict adherence to safety invariants and explicit memory ownership.

---

## 📂 Project Structure

```text
mm-notebook/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs        # Public library entry point
    ├── index.rs      # Zone partitioning, layout mapping, and policy router
    ├── notebook.rs   # Core arena lifecycle (`mmap`, `munmap`, Drop semantics)
    ├── sighandler.rs # Signal installation, alternate stack, and `SIGSEGV` router
    ├── telemetry.rs  # Async-signal-safe metrics and audit report generator
    └── workload.rs   # Synthetic test workloads (Lazy, Read-Only, Guard)
```

---

## 🚀 Installation & Usage

### Prerequisites
* Linux operating system (x86_64)
* Rust toolchain (stable)

### Building from Source
```bash
git clone https://github.com/Aijazbir2010/mm-notebook.git
cd mm-notebook
cargo build --release
```

### Running the CLI (`mmnb`)

* **Test Lazy Allocation (On-Demand Page Population):**
  ```bash
  cargo run --bin mmnb -- --mode lazy
  ```

* **Test Read-Only Zone Protection (Security Enforcement):**
  ```bash
  cargo run --bin mmnb -- --mode read
  ```

* **Test Guard Zone Boundary Trap (Fatal Abort & Forensics):**
  ```bash
  cargo run --bin mmnb -- --mode guard
  ```

---

## 📊 Sample Telemetry Reports

### Lazy Mode Execution Report
```text
=============================================
 MMNotebook Execution Telemetry Report
=============================================
 Notebook Size        : 1,073,741,824 bytes (1.00 GiB)
 Page Size            : 4096 bytes
 Total Pages Managed  : 262,144
---------------------------------------------
 Faults Triggered     : 12
   - Lazy Allocations : 12 pages populated on demand
 Time in Handler      : 42,150 nanoseconds total
=============================================
```

### Security Incident Audit Report (Guard / Read-Only Violation)
```text
=============================================
 MMNotebook Security Audit Report
=============================================
 Faults Triggered     : 1
---------------------------------------------
 [CRITICAL INCIDENT]
 - Fault Address      : 0x7f9a2b400000
 - Fault Type         : Unauthorized access on Guard Page
 - Faulting RIP       : 0x00000000004012ab
 - Action Taken       : Terminated process (SIGSEGV Aborted)
=============================================
```

---

## 🛠️ Tech Stack
* **Language:** Rust
* **System APIs:** `mmap`, `munmap`, `mprotect`, `sigaction`, `sigaltstack`, `clock_gettime`
* **Crates:** `rustix`, `libc`, `clap`

---

## 📄 License
Distributed under the MIT License.