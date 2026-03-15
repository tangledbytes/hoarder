#![no_std]

extern crate alloc;

use alloc::sync::Arc; // ??
use core::sync::atomic::{AtomicPtr, AtomicU16, AtomicUsize, Ordering};

use hoarder_collections::alloc::AlignedBuffers;
use hoarder_common::error::{HoarderError, Result};

const WRITER_BIT: u16 = 1 << 8;
const INDEX_MASK: u16 = 1;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Print = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl LogLevel {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Print => "",
            Self::Debug => "[DEBUG] ",
            Self::Info => "[INFO] ",
            Self::Warn => "[WARN] ",
            Self::Error => "[ERROR] ",
        }
    }
}

pub static LOGGER_PTR: AtomicPtr<Producer> = AtomicPtr::new(core::ptr::null_mut());

/// Initialize the global logger. Must be called once before any logging
/// macros. The producer must have `'static` lifetime (e.g. via `Box::leak`).
pub fn init_logger(producer: &'static Producer) {
    LOGGER_PTR.store(
        producer as *const Producer as *mut Producer,
        Ordering::SeqCst,
    );
}

/// A fixed-capacity formatting buffer backed by a borrowed `&mut [u8]`.
/// Implements `core::fmt::Write`; silently truncates on overflow.
pub struct FmtBuf<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> FmtBuf<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.pos]
    }

    pub fn clear(&mut self) {
        self.pos = 0;
    }

    pub fn len(&self) -> usize {
        self.pos
    }
}

impl core::fmt::Write for FmtBuf<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let available = self.buf.len() - self.pos;
        let to_write = bytes.len().min(available);
        self.buf[self.pos..self.pos + to_write].copy_from_slice(&bytes[..to_write]);
        self.pos += to_write;
        Ok(())
    }
}

// ── Logger / Producer / Consumer ───────────────────────────────────────

pub struct Logger {
    bufs: [AlignedBuffers; 2],
    offsets: [AtomicUsize; 2],
    state: State,
}

// SAFETY: The `State` atomic variable guarantees that the Producer and Consumer
// never concurrently access the same index of `bufs` or `offsets`.
// Memory visibility is strictly maintained via Acquire/Release orderings.
unsafe impl Send for Logger {}
unsafe impl Sync for Logger {}

pub struct Producer {
    shared: Arc<Logger>,
    buf_size: usize,
}

pub struct Consumer {
    shared: Arc<Logger>,
}

impl Logger {
    pub fn new(buf_size: usize) -> Result<(Producer, Consumer)> {
        assert!(buf_size >= 0x1000);
        let bufs = [
            AlignedBuffers::new(1, buf_size, 0x1000)?,
            AlignedBuffers::new(1, buf_size, 0x1000)?,
        ];
        let offsets = [AtomicUsize::new(0), AtomicUsize::new(0)];

        let logger = Arc::new(Logger {
            bufs,
            offsets,
            state: State(AtomicU16::new(0)),
        });

        Ok((
            Producer {
                shared: Arc::clone(&logger),
                buf_size,
            },
            Consumer { shared: logger },
        ))
    }
}

impl Producer {
    pub fn push(&self, data: &[u8]) -> Result<()> {
        loop {
            let curr_state = self.shared.state.0.load(Ordering::Relaxed);
            let active_buffer_idx = curr_state & INDEX_MASK;
            assert!(active_buffer_idx <= 1);
            let new_state = WRITER_BIT | active_buffer_idx;

            if self
                .shared
                .state
                .0
                .compare_exchange_weak(curr_state, new_state, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // If the data cannot fit in the buffer then simply return
                let offset =
                    self.shared.offsets[active_buffer_idx as usize].load(Ordering::Acquire);
                if offset
                    .checked_add(data.len())
                    .map_or(true, |end| end > self.buf_size)
                {
                    self.shared
                        .state
                        .0
                        .fetch_and(!WRITER_BIT, Ordering::Release);
                    return Err(HoarderError::PushError);
                }

                // If the data fits in the buffer then write it
                unsafe {
                    core::slice::from_raw_parts_mut(
                        self.shared.bufs[active_buffer_idx as usize].buf_ptr_mut(0),
                        self.buf_size,
                    )[offset..][..data.len()]
                        .copy_from_slice(data)
                };

                self.shared.offsets[active_buffer_idx as usize]
                    .fetch_add(data.len(), Ordering::Release);

                // Once done writing, revert the active writer state
                // so that the consumer can consume the buffer when
                // needed
                self.shared
                    .state
                    .0
                    .fetch_and(!WRITER_BIT, Ordering::Release);
                return Ok(());
            }
        }
    }

    /// panic_flush will forcefully read the data from the buffers and
    /// is intended to be used by panic hooks to dump data before crashing
    pub fn panic_flush(&self, mut cb: impl FnMut(&[u8])) {
        for idx in 0..=1 {
            // SAFE: Using atomic load. No data races!
            let offset = self.shared.offsets[idx].load(Ordering::Relaxed);

            if offset > 0 {
                // SAFE: Concurrent reads to the buffer are fine, and we now
                // have a safely acquired offset boundary.
                let data = unsafe {
                    core::slice::from_raw_parts(self.shared.bufs[idx].buf_ptr(0), offset)
                };
                cb(data);
            }
        }
    }
}

impl Consumer {
    pub fn consume(&self, mut cb: impl FnMut(&[u8])) {
        loop {
            let curr_state = self.shared.state.0.load(Ordering::Acquire);

            // Wait patiently without thrashing the memory bus
            if (curr_state & WRITER_BIT) != 0 {
                core::hint::spin_loop();
                continue;
            }

            let active_buffer_idx = curr_state & INDEX_MASK;
            assert!(active_buffer_idx <= 1);
            let new_active_buffer_idx = 1 - active_buffer_idx;
            let new_state: u16 = INDEX_MASK & new_active_buffer_idx;

            if self
                .shared
                .state
                .0
                .compare_exchange_weak(curr_state, new_state, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                // Load offset of the previously active buffer
                let offset =
                    self.shared.offsets[active_buffer_idx as usize].load(Ordering::Acquire);

                if offset > 0 {
                    cb(unsafe {
                        core::slice::from_raw_parts(
                            self.shared.bufs[active_buffer_idx as usize].buf_ptr(0),
                            offset,
                        )
                    });
                }

                // Reset the offset of the consumed buffer
                self.shared.offsets[active_buffer_idx as usize].store(0, Ordering::Release);
                return;
            }
        }
    }
}

struct State(AtomicU16);

/// Maximum size of a single formatted log line (stack-allocated).
pub const MAX_LOG_LINE: usize = 512;

/// Internal macro — do not call directly.
#[macro_export]
macro_rules! __hlog {
    ($level:expr, $($args:tt)*) => {{
        let producer_ptr = $crate::LOGGER_PTR.load(core::sync::atomic::Ordering::Relaxed);
        if !producer_ptr.is_null() {
            let producer = unsafe { &*producer_ptr };
            let mut raw = [0u8; $crate::MAX_LOG_LINE];
            let mut fmt = $crate::FmtBuf::new(&mut raw);
            let _ = <$crate::FmtBuf as core::fmt::Write>::write_str(&mut fmt, $level.prefix());
            let _ = <$crate::FmtBuf as core::fmt::Write>::write_fmt(
                &mut fmt,
                core::format_args!($($args)*),
            );
            let _ = <$crate::FmtBuf as core::fmt::Write>::write_str(&mut fmt, "\n");
            let _ = producer.push(fmt.as_bytes());
        }
    }};
}

#[macro_export]
macro_rules! hprint {
    ($($args:tt)*) => { $crate::__hlog!($crate::LogLevel::Print, $($args)*) };
}

#[macro_export]
macro_rules! hdebug {
    ($($args:tt)*) => { $crate::__hlog!($crate::LogLevel::Debug, $($args)*) };
}

#[macro_export]
macro_rules! hinfo {
    ($($args:tt)*) => { $crate::__hlog!($crate::LogLevel::Info, $($args)*) };
}

#[macro_export]
macro_rules! hwarn {
    ($($args:tt)*) => { $crate::__hlog!($crate::LogLevel::Warn, $($args)*) };
}

#[macro_export]
macro_rules! herror {
    ($($args:tt)*) => { $crate::__hlog!($crate::LogLevel::Error, $($args)*) };
}

#[cfg(test)]
mod test {
    extern crate alloc;
    use crate::{Logger, init_logger};
    use alloc::boxed::Box;

    #[test]
    fn test_sanity() {
        let (producer, consumer) = Logger::new(0x1000).unwrap();
        producer.push("hello world".as_bytes()).unwrap();
        consumer.consume(|buf| {
            assert_eq!(buf, "hello world".as_bytes());
        });
    }

    #[test]
    fn test_log_roundtrip() {
        let (producer, consumer) = Logger::new(0x1000).unwrap();

        // Leak producer to get a 'static reference for init_logger
        let producer = Box::leak(Box::new(producer));
        init_logger(producer);

        hinfo!("count = {}, name = {}", 42u32, "alice");

        let mut output = [0u8; 1024];
        let mut output_len = 0;
        consumer.consume(|buf| {
            output[..buf.len()].copy_from_slice(buf);
            output_len = buf.len();
        });

        let text = core::str::from_utf8(&output[..output_len]).unwrap();
        assert_eq!(text, "[INFO] count = 42, name = alice\n");

        crate::LOGGER_PTR.store(core::ptr::null_mut(), core::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn test_consume_empty() {
        let (_producer, consumer) = Logger::new(0x1000).unwrap();
        let mut called = false;
        consumer.consume(|_| {
            called = true;
        });
        assert!(!called);
    }

    #[test]
    fn test_multiple_logs() {
        let (producer, consumer) = Logger::new(0x1000).unwrap();
        let producer = Box::leak(Box::new(producer));
        init_logger(producer);

        hinfo!("first = {}", 1u32);
        hwarn!("second = {}", 2u32);
        herror!("third");

        let mut output = [0u8; 4096];
        let mut output_len = 0;
        consumer.consume(|buf| {
            output[..buf.len()].copy_from_slice(buf);
            output_len = buf.len();
        });

        let text = core::str::from_utf8(&output[..output_len]).unwrap();
        assert!(text.contains("[INFO] first = 1\n"));
        assert!(text.contains("[WARN] second = 2\n"));
        assert!(text.contains("[ERROR] third\n"));

        crate::LOGGER_PTR.store(core::ptr::null_mut(), core::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn test_debug_format() {
        let (producer, consumer) = Logger::new(0x1000).unwrap();
        let producer = Box::leak(Box::new(producer));
        init_logger(producer);

        #[derive(Debug)]
        struct Point {
            x: i32,
            y: i32,
        }

        let p = Point { x: 10, y: 20 };
        hdebug!("point = {:?}", p);

        let mut output = [0u8; 1024];
        let mut output_len = 0;
        consumer.consume(|buf| {
            output[..buf.len()].copy_from_slice(buf);
            output_len = buf.len();
        });

        let text = core::str::from_utf8(&output[..output_len]).unwrap();
        assert_eq!(text, "[DEBUG] point = Point { x: 10, y: 20 }\n");

        crate::LOGGER_PTR.store(core::ptr::null_mut(), core::sync::atomic::Ordering::SeqCst);
    }
}
