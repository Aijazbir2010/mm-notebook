use mm_notebook::{Notebook, Zone};
use mm_notebook::workload;
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(version, about, override_usage = "mmnb --mode <MODE>")]
struct Args {
    /// Operating Mode
    #[arg(short, long, value_enum)]
    mode: Mode
}

#[derive(Clone, ValueEnum, Debug)]
enum Mode {
    /// Prints security report and terminates process
    Read,
    /// Writes to lazy zone and prints execution report
    Lazy,
    /// Prints security report and terminates process
    Guard,
}

fn main() {
    let args = Args::parse();
    
    let notebook = Notebook::new().expect("Failed to create notebook!");

    match args.mode {
        Mode::Read => {
            let metadata = notebook.get_zone_metadata(Zone::ReadOnly);
            unsafe { workload::rozone_write(metadata.ptr); }
        }
        Mode::Lazy => {
            let metadata = notebook.get_zone_metadata(Zone::Lazy);
            unsafe { let _ = workload::lzone_write(metadata.ptr, 12 * 4096); }
            mm_notebook::print_audit_report();
        }
        Mode::Guard => {
            let metadata = notebook.get_zone_metadata(Zone::Guard);
            unsafe { workload::gzone_write(metadata.ptr); }
        }
    }
}
