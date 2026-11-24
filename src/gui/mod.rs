pub mod native;
pub mod server;

pub use native::run_native_gui;
pub use server::run_gui as run_web_gui;
