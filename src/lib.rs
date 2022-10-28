#[macro_use] extern crate tracing;

pub mod api;
pub mod formats;
pub mod service;
pub mod standard;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::prelude::*;

    let filter = EnvFilter::from_default_env();
    let fmt = tracing_subscriber::fmt::layer()
        .with_target(false)
        .compact();
    let tracing = tracing_subscriber::registry()
        .with(filter)
        .with(fmt)
        .try_init();

    if let Err(err) = tracing {
        warn!(%err, "tracing failed to initialize");
    } else {
        tracing.unwrap();
    }
}
