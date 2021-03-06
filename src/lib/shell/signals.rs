//! This module contains all of the code that manages signal handling in the
//! shell. Primarily, this will be used to block signals in the shell at
//! startup, and unblock signals for each of the forked
//! children of the shell.

// use std::sync::atomic::{ATOMIC_U8_INIT, AtomicU8};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::sys;

pub(crate) use crate::sys::signals::{block, unblock};

pub static PENDING: AtomicUsize = AtomicUsize::new(0);
pub const SIGINT: u8 = 1;
pub const SIGHUP: u8 = 2;
pub const SIGTERM: u8 = 4;

/// Suspends a given process by it's process ID.
pub(crate) fn suspend(pid: u32) { let _ = sys::killpg(pid, sys::SIGSTOP); }

/// Resumes a given process by it's process ID.
pub(crate) fn resume(pid: u32) { let _ = sys::killpg(pid, sys::SIGCONT); }

/// The purpose of the signal handler is to ignore signals when it is active, and then continue
/// listening to signals once the handler is dropped.
pub(crate) struct SignalHandler;

impl SignalHandler {
    pub(crate) fn new() -> SignalHandler {
        block();
        SignalHandler
    }
}

impl Drop for SignalHandler {
    fn drop(&mut self) { unblock(); }
}

impl Iterator for SignalHandler {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        match PENDING.swap(0, Ordering::SeqCst) as u8 {
            0 => None,
            SIGINT => Some(sys::SIGINT),
            SIGHUP => Some(sys::SIGHUP),
            SIGTERM => Some(sys::SIGTERM),
            _ => unreachable!(),
        }
    }
}
