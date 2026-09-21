//! A [RuntimeFlavor](crate::runtime::RuntimeFlavor) using OS threads to provision workers
mod communication;
mod multi;
mod single;

pub use multi::MultiThreadRuntime;
pub use single::SingleThreadRuntime;
pub use single::SingleThreadRuntimeFlavor;
