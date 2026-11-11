//! SIGTERM, SIGINT and SIGHUP: wake the event loop so it can exit through the
//! normal quit path (saving the playlist) instead of being killed mid-frame.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use winit::event_loop::EventLoopProxy;

/// The returned flag is set when a termination signal arrives. A second signal
/// exits at once, for a loop that is stuck and cannot honour the first.
pub fn watch(wake: EventLoopProxy<()>) -> Arc<AtomicBool> {
    let requested = Arc::new(AtomicBool::new(false));
    let mut signals = match Signals::new([SIGTERM, SIGINT, SIGHUP]) {
        Ok(signals) => signals,
        Err(err) => {
            tracing::warn!("cannot handle termination signals: {err}");
            return requested;
        }
    };
    let flag = Arc::clone(&requested);
    let spawned = std::thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            for signal in signals.forever() {
                if flag.swap(true, Ordering::SeqCst) {
                    std::process::exit(128 + signal);
                }
                let _ = wake.send_event(());
            }
        });
    if let Err(err) = spawned {
        tracing::warn!("cannot handle termination signals: {err}");
    }
    requested
}
