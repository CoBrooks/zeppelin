pub mod api;
pub mod formats;
pub mod service;
pub mod standard;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
