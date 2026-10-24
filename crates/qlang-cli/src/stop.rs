//! `Process.stopping()`: a program that wants to leave cleanly asks whether it was told to stop.
//! The first call takes over the signals (Ctrl-C, `kill`): they set a flag instead of ending the
//! program; a second one ends it. A server has its own handling and shares its flag.

use std::sync::atomic::{AtomicBool, Ordering};

static FLAG: AtomicBool = AtomicBool::new(false);
static ARMED: AtomicBool = AtomicBool::new(false);
/// Set by `qlang serve`, which installs its own handler.
static SERVING: AtomicBool = AtomicBool::new(false);

pub fn serving() {
    SERVING.store(true, Ordering::SeqCst);
}

pub fn stopping(server_stopping: bool) -> bool {
    if !SERVING.load(Ordering::SeqCst) && !ARMED.swap(true, Ordering::SeqCst) {
        let _ = ctrlc::set_handler(|| {
            // the second signal is an order
            if FLAG.swap(true, Ordering::SeqCst) {
                std::process::exit(130);
            }
        });
    }
    FLAG.load(Ordering::SeqCst) || server_stopping
}
