use crate::error::{HoarderError, Result};
use io_uring::{self, IoUring, cqueue::Entry as CQE, squeue::Entry as SQE};

/// IO trait is a generic trait which intentionally resembles io_uring
/// API and is intended to in general encompass IO which takes
/// requests in a submission queue and posts completions in the completion
/// queue.
pub trait IO {
    /// completion returns an iterator over the completion queue
    /// of the underlying IO.
    fn completion(&mut self) -> impl Iterator<Item = CQE>;

    /// enqueue should push the given submission queue entry
    /// to the submission queue of the IO. It will return
    /// error if the submission queue is full.
    fn enqueue(&mut self, sqe: &SQE) -> Result<()>;

    /// submit all the batched requests to the underlying
    /// environment and wait for at least `want` requests
    /// to be completed.
    fn submit_and_wait(&mut self, want: usize) -> Result<usize>;

    /// Registers an empty file table of nr_files number of file descriptors.
    /// Registering a file table is a prerequisite for using any request
    /// that uses direct descriptors.
    fn register_files(&mut self, nr: u32) -> Result<()>;

    /// Register in-memory fixed buffers for I/O with the kernel.
    unsafe fn register_buffers(&mut self, bufs: &[libc::iovec]) -> Result<()>;
}

/// UringIO is intended as a concrete implemention of the IO
/// trait and basically is a thin wrapper around raw io_uring
/// API.
pub struct UringIO {
    ring: IoUring<SQE, CQE>,
}

impl UringIO {
    pub fn new(entries: u32, use_sqpoll: bool) -> Result<Self> {
        let mut builder = IoUring::builder();
        if use_sqpoll {
            builder.setup_sqpoll(2000);
        }

        let ring = builder
            .build(entries)
            .map_err(|e| HoarderError::from(e.raw_os_error().unwrap()))?;
        Ok(Self { ring })
    }
}

impl IO for UringIO {
    fn completion(&mut self) -> impl Iterator<Item = CQE> {
        self.ring.completion()
    }

    fn enqueue(&mut self, sqe: &SQE) -> Result<()> {
        unsafe { self.ring.submission().push(sqe) }.or(Err(HoarderError::PushError))?;
        Ok(())
    }

    fn submit_and_wait(&mut self, want: usize) -> Result<usize> {
        self.ring.completion().sync();

        let cqsize = self.ring.completion().len();
        if !self.ring.submission().need_wakeup() && cqsize >= want {
            return Ok(cqsize);
        }

        let got = self
            .ring
            .submit_and_wait(want)
            .map_err(|e| HoarderError::from(e.raw_os_error().unwrap()))?;
        Ok(got)
    }

    fn register_files(&mut self, nr: u32) -> Result<()> {
        let _ = self
            .ring
            .submitter()
            .register_files_sparse(nr)
            .map_err(|e| HoarderError::from(e.raw_os_error().unwrap()))?;
        Ok(())
    }

    unsafe fn register_buffers(&mut self, bufs: &[libc::iovec]) -> Result<()> {
        let _ = unsafe { self.ring.submitter().register_buffers(bufs) }
            .map_err(|e| HoarderError::from(e.raw_os_error().unwrap()))?;
        Ok(())
    }
}
