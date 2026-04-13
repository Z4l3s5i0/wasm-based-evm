#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    None = 0,
    Info = 1,
    Debug = 2,
}

pub static mut LOG_LEVEL: LogLevel = LogLevel::Info;

pub fn set_log_level(level: LogLevel) {
    unsafe {
        LOG_LEVEL = level;
    }
}

pub fn get_log_level() -> LogLevel {
    unsafe { LOG_LEVEL }
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if $crate::logging::get_log_level() >= $crate::logging::LogLevel::Info {
            println!($($arg)*)
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::logging::get_log_level() >= $crate::logging::LogLevel::Debug {
            println!($($arg)*)
        }
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}
