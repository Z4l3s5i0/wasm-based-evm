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
                let now = chrono::Local::now();
                let timestamp = now.format("%b %d %H:%M:%S%.3f").to_string();
                let log = format!($($arg)*);
                println!("{} INFO  {}", timestamp, log);
                $crate::logging::add_log(format!("{} [INFO] {}", timestamp, log));
            }
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        {
            if $crate::logging::get_log_level() >= $crate::logging::LogLevel::Debug {
                let now = chrono::Local::now();
                let timestamp = now.format("%b %d %H:%M:%S%.3f").to_string();
                let log = format!($($arg)*);
                println!("{} DEBUG {}", timestamp, log);
                $crate::logging::add_log(format!("{} [DEBUG] {}", timestamp, log));
            }
        }
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        {
            let now = chrono::Local::now();
            let timestamp = now.format("%b %d %H:%M:%S%.3f").to_string();
            let log = format!($($arg)*);
            eprintln!("{} ERROR {}", timestamp, log);
            $crate::logging::add_log(format!("{} [ERROR] {}", timestamp, log));
        }
    };
}
