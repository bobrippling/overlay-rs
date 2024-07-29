#![no_std]
#![warn(missing_docs)]

pub trait Overlay {
    fn overlay(bytes: &[u8]) -> Result<&Self, Error>;
    fn overlay_mut(bytes: &mut [u8]) -> Result<&mut Self, Error>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    InsufficientLength,
}
