#![no_std]
#![doc = include_str!("../README.md")]

#[macro_use]
extern crate log;

pub mod generic_timer;
#[cfg(not(feature = "gicv3"))]
pub mod gic;
#[cfg(feature = "gicv3")]
pub mod gicv3;
pub mod pl011;
pub mod pl031;
pub mod psci;
