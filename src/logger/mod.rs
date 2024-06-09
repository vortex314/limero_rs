use std::io::Write;
use std::thread;
use log::{info, warn, error};

pub fn init() {
    let mut builder = env_logger::Builder::from_default_env();
    builder
        .format(|buf, record| {
            let thread_name = thread::current();
            let name = thread_name.name().unwrap_or("unknown");
            writeln!(
                buf,
                "[{}] {:10.10} | {:12.12}:{:3}| {} {}",
                chrono::Local::now().format("%H:%M:%S.%3f"),
                name,
                record.file().unwrap_or("unknown").rsplit_once('/').unwrap().1,
                record.line().unwrap_or(0),
                record.level(),
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Info)
        .init();
    info!("Logger initialized"  );
}