use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::util::SubscriberInitExt;

pub mod filesystem;

use filesystem::Daniel;

fn main() {
    let _ = tracing_subscriber::FmtSubscriber::builder()
        .with_ansi(true)
        .with_max_level(LevelFilter::INFO)
        .with_span_events(FmtSpan::ENTER)
        .finish()
        .try_init();

    if !std::fs::exists("/tmp/daniel/").unwrap() {
        _ = std::fs::create_dir("/tmp/daniel/");
    }

    let mountpoint = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            println!("Usage: {} <MOUNTPOINT>", std::env::args().next().unwrap());
            return;
        }
    };

    let fs = Daniel::new();

    fuser::mount2(fs, &mountpoint, &[]).expect("Couldn't mount filesystem");
}
