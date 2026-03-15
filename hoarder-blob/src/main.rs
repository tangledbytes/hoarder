use hoarder_blob::executor::Executor;
use hoarder_common::error::Result;
use hoarder_io::UringIO;
use hoarder_log::{Logger, init_logger};

fn main() -> Result<()> {
    let (producer, consumer) = Logger::new(512 * 1000)?;

    // Leak the producer to obtain a 'static reference for the global
    let producer = Box::leak(Box::new(producer));
    init_logger(producer);

    std::thread::spawn(move || {
        loop {
            consumer.consume(|buf| {
                unsafe { libc::write(libc::STDOUT_FILENO, buf.as_ptr() as _, buf.len() as _) };
            });

            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });

    hoarder_log::hinfo!("blob engine starting");

    let uring = UringIO::new(4096, false).unwrap();
    let mut executor = Executor::new(uring, 1, 100, 100);

    hoarder_log::hinfo!("executor initialized, entering main loop");
    executor.run();

    Ok(())
}
