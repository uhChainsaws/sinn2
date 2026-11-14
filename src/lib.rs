#![no_std]

#[cfg(test)]
extern crate std;

pub mod audio;
pub mod codec;

// Named `dma` to satisfy the unmodified upstream ring buffer's crate::dma import.
#[cfg(test)]
#[path = "tests/dma.rs"]
mod dma;
