use hoarder_blob::executor::Executor;
use hoarder_common::error::Result;
use hoarder_io::UringIO;
use hoarder_log::{Consumer, Logger, Producer, init_logger};

fn write_to_stdout(buf: &[u8]) {
    unsafe { libc::write(libc::STDOUT_FILENO, buf.as_ptr() as _, buf.len() as _) };
}

fn setup_log_consumer(consumer: Consumer) {
    std::thread::spawn(move || {
        loop {
            consumer.consume(write_to_stdout);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });
}

fn setup_panic_hook(producer: &'static Producer) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        producer.panic_flush(write_to_stdout);
        default_hook(panic_info);
    }));
}

fn main() -> Result<()> {
    let (producer, consumer) = Logger::new(512 * 1000)?;

    // Leak the producer to obtain a 'static reference for the global
    let producer = Box::leak(Box::new(producer));
    setup_panic_hook(producer);
    init_logger(producer);
    setup_log_consumer(consumer);

    hoarder_log::hinfo!("blob engine starting");

    let uring = UringIO::new(4096, false).unwrap();
    let mut executor = Executor::new(uring, 1, 100, 100);

    hoarder_log::hinfo!("executor initialized, entering main loop");
    executor.run();

    Ok(())
}
