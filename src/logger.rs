//! A simple logger implementation that writes to the terminal

use std::io::{self, Write};
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;


// The following global sender is for convenience only. Without it, we should have passed a Sender
// clone into every struct, function, and background task that needed to log something. By making
// the Sender global, we could more easily build macros like info!() and error!() that magically
// work anywhere in the codebase.
//
// Since the Sender is initialized at runtime, we needed to pair the static GLOBAL_SENDER with
// the OnceLock solution. It provides thread-safe initialization and lock-free atomic access.
//
static GLOBAL_SENDER: OnceLock<Sender<LogMessage>> = OnceLock::new();

/// The severity of the log record
#[allow(dead_code)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[allow(dead_code)]
enum LogMessage {
    /* events   */ Record(String, Severity, SystemTime),
    /* commands */ Flush, Shutdown,
}


/// Flushes and waits for the logger thread to finish when main exits
pub struct LoggerGuard {
    tx: Sender<LogMessage>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for LoggerGuard {
    fn drop(&mut self) {
        // Flush any eventual messages still in stdio internal buffers,
        // and command the Shutdown so to break the receiver loop
        let _ = self.tx.send(LogMessage::Flush);
        let _ = self.tx.send(LogMessage::Shutdown);

        // Join the logger thread to guarantee every queued message hits stdout
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn format_record(text: String, severity: Severity, ts: SystemTime) -> String {
    let tag = match severity {
        Severity::Info  => INFO_TAG,
        Severity::Warn  => WARN_TAG,
        Severity::Error => ERROR_TAG,
    };
    let rfc3339 = humantime::format_rfc3339_seconds(ts).to_string();
    format!("{} | {} | {}", tag, rfc3339, text)
}

fn receiver_loop(rx: Receiver<LogMessage>) {
    // Lock once for maximum I/O performance
    let mut stdio = io::stdout().lock();

    while let Ok(msg) = rx.recv() {
        match msg {
            LogMessage::Record(text, severity, ts) => {
                let formatted = format_record(text, severity, ts);
                let _ = writeln!(stdio, "{}", formatted);
            }
            LogMessage::Flush => {
                let _ = stdio.flush();
            }
            LogMessage::Shutdown => {
                break // <--- This exits the loop, ending the logger thread
            }
        }
    }
}


/// Initialize the logger.
///
/// Since our logger writes message to the terminal, and since this performed by the underlying OS
/// as blocking operation, our logger is **not** implemented using the Tokio asynchronous runtime.
/// We are not spawning Tokio tasks as loggers.
///
/// Rather, we are using a standard Multi-Producers Single-Consumer (MPSC) channel and a dedicated
/// OS-thread which runs the receiving loop until the Shutdown message is received.
///
pub fn init() -> LoggerGuard {

    // Check upfront to avoid spawning a receiver thread if already initialized
    assert!(
        GLOBAL_SENDER.get().is_none(),
        "Attempted to initialize the global logger more than once!"
    );

    // Create the MPSC channel
    let (tx, rx) = mpsc::channel::<LogMessage>();

    // Spawns the log events receiver.
    // This is a dedicated OS-thread which keeps receiving log events in a loop.
    let handle = thread::spawn(|| receiver_loop(rx));

    // This will now always succeed because of the check above,
    // but we still use set() to satisfy Rust's thread-safety compiler guarantees.

    if GLOBAL_SENDER.set(tx.clone()).is_err() {
        unreachable!("Checked above, but handled safely just in case");
    }

    LoggerGuard { tx, handle: Some(handle) }
}


const INFO_TAG: &str = "\x1b[32m\x1b[1mINFO\x1b[0m ";
const WARN_TAG: &str = "\x1b[33m\x1b[1mWARN\x1b[0m ";
const ERROR_TAG: &str = "\x1b[31m\x1b[1mERROR\x1b[0m";


/// Send helper used by the macros
#[allow(dead_code)]
pub fn send_record(severity: Severity, text: String) {
    // Note that accessing a OnceLock via .get() does not require acquiring a Mutex.
    // It is a lock-free atomic read
    if let Some(tx) = GLOBAL_SENDER.get() {
        let _ = tx.send(LogMessage::Record(text, severity, SystemTime::now()));
    }
    else {
        // Fallback to standard println if logger isn't initialized yet
        println!("{}", text);
    }
}


// ----- MACROs in production -----

#[cfg(not(test))]
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logger::send_record($crate::logger::Severity::Info, format!($($arg)*))
    };
}

#[cfg(not(test))]
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::logger::send_record($crate::logger::Severity::Warn, format!($($arg)*))
    };
}

#[cfg(not(test))]
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logger::send_record($crate::logger::Severity::Error, format!($($arg)*))
    };
}


// ----- MACROs in test -----

// Note that in test builds the logging macros must stay silent, but they must
// NOT expand to nothing: any variable referenced only inside a log call (e.g.
// `info!("{version}")`) would otherwise be reported as an unused variable by
// `cargo test`, even though the very same code compiles warning-free under
// `cargo build` (where the production macros consume those arguments).
// Wrapping the format in `if false { .. }` keeps the arguments type-checked and
// referenced while the optimizer removes the branch entirely at runtime.

#[cfg(test)]
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if false {
            let _ = format!($($arg)*);
        }
    };
}

#[cfg(test)]
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        if false {
            let _ = format!($($arg)*);
        }
    };
}

#[cfg(test)]
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        if false {
            let _ = format!($($arg)*);
        }
    };
}