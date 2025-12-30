#![no_std]
#![doc = include_str!("../README.md")]

#[macro_use]
extern crate log;

#[cfg(all(feature = "irq", not(feature = "gicv3")))]
pub mod gic;
#[cfg(all(feature = "irq", feature = "gicv3"))]
pub mod gicv3;

pub mod generic_timer;
pub mod pl011;
pub mod pl031;
pub mod psci;
