use core::fmt::{self, Debug};

pub type Result<T> = core::result::Result<T, HoarderError>;

#[derive(Debug)]
pub enum HoarderError {
    PushError,
    MemAllocFail,
    BufferAllocFail,
    IoError(i32),
}

impl fmt::Display for HoarderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PushError => write!(f, "failed to push entry to queue"),
            Self::MemAllocFail => write!(f, "failed to allocate memory"),
            Self::BufferAllocFail => write!(f, "failed to allocate buffer"),
            Self::IoError(errno) => write!(f, "os error: {}", errno),
        }
    }
}

impl From<i32> for HoarderError {
    fn from(value: i32) -> Self {
        Self::IoError(value)
    }
}
