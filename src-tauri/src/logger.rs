use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Mutex, OnceLock};

static LOG: OnceLock<Mutex<BufWriter<File>>> = OnceLock::new();

pub fn init(path: &std::path::Path) {
    match OpenOptions::new().create(true).write(true).truncate(true).open(path) {
        Ok(f) => {
            let _ = LOG.set(Mutex::new(BufWriter::new(f)));
            eprintln!("[log] Writing to: {}", path.display());
        }
        Err(e) => eprintln!("[log] Could not open log file {:?}: {}", path, e),
    }
}

pub fn write_line(msg: &str) {
    eprintln!("{}", msg);
    if let Some(lock) = LOG.get() {
        if let Ok(mut w) = lock.lock() {
            let _ = writeln!(w, "{}", msg);
            let _ = w.flush();
        }
    }
}

#[macro_export]
macro_rules! mwlog {
    ($($arg:tt)*) => {
        $crate::logger::write_line(&format!($($arg)*))
    };
}
