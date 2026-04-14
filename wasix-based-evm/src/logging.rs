use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    None = 0,
    Info = 1,
    Debug = 2,
}

pub static mut LOG_LEVEL: LogLevel = LogLevel::Info;

pub static LOGS: Lazy<Arc<Mutex<VecDeque<String>>>> = Lazy::new(|| Arc::new(Mutex::new(VecDeque::with_capacity(1000))));

pub fn set_log_level(level: LogLevel) {
    unsafe {
        LOG_LEVEL = level;
    }
}

pub fn get_log_level() -> LogLevel {
    unsafe { LOG_LEVEL }
}

pub fn add_log(log: String) {
    let mut logs = LOGS.lock().unwrap();
    if logs.len() >= 1000 {
        logs.pop_front();
    }
    logs.push_back(log);
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        {
            if $crate::logging::get_log_level() >= $crate::logging::LogLevel::Info {
                let log = format!($($arg)*);
                println!("{}", log);
                $crate::logging::add_log(format!("[INFO] {}", log));
            }
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        {
            if $crate::logging::get_log_level() >= $crate::logging::LogLevel::Debug {
                let log = format!($($arg)*);
                println!("{}", log);
                $crate::logging::add_log(format!("[DEBUG] {}", log));
            }
        }
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        {
            let log = format!($($arg)*);
            eprintln!("{}", log);
            $crate::logging::add_log(format!("[ERROR] {}", log));
        }
    };
}
