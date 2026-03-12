use thiserror::Error;
pub use rustix::io::Errno;

pub type Result<T> = core::result::Result<T, HoarderError>;

#[derive(Error, Debug)]
pub enum HoarderError {
	#[error("failed to push entry to queue")]
	PushError,

	#[error("fnvalid subsystem ID")]
	MachineType,

    #[error("failed to allocate memory")]
    MemAllocFail,

    #[error("failed to allocate buffer")]
    BufferAllocFail,

    #[error("os error {}", .0.raw_os_error())]
	IoError(rustix::io::Errno)
}

impl From<rustix::io::Errno> for HoarderError {
    fn from(value: rustix::io::Errno) -> Self {
        HoarderError::IoError(value)
    }
}

impl From<i32> for HoarderError {
    fn from(value: i32) -> Self {
        rustix::io::Errno::from_raw_os_error(-value).into()
    }
}