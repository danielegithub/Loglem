pub mod access;
pub mod archive;
pub mod cli;
pub mod collector;
pub mod config;
pub mod diagnostics;
pub mod management;
pub mod model;
pub mod process;
pub mod runtime;
pub mod service;
pub mod sources;
pub mod tail;
pub mod terminal;
pub mod timestamps;
pub mod update;
pub mod util;
pub mod web;

#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
pub const BATCH_SIZE: usize = 256;
