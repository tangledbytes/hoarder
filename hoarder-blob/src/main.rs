use hoarder_blob::executor::Executor;
use hoarder_common::error::Result;
use hoarder_io::UringIO;

fn main() -> Result<()> {
    #[cfg(debug_assertions)]
    {
        env_logger::init();
        log::info!("Logger initialized in dev mode");
    }

    let uring = UringIO::new(4096, false).unwrap();
    let mut executor = Executor::new(uring, 1, 100, 100);
    executor.run();

    Ok(())
}
