//! Measures what DESIGN.md §12 leaves open about the hardware: which cores an app may start
//! threads on, how fast each runs for it, and how much memory it gets.
//!
//! Rust's std can't choose a thread's core on the 3DS, so the threads here are libctru's.

use std::ffi::c_void;
use std::hint::black_box;

use ctru::os::MemRegion;
use ctru::prelude::*;

/// Iterations of the timed loop: long enough to time, short enough not to stall startup.
const WORK: u32 = 2_000_000;
/// libctru's thread priorities run 0x18 (highest) to 0x3F; an app's main thread has 0x30.
const PRIORITY: i32 = 0x31;
const STACK: usize = 0x4000;
/// The share of the system core (core 1) asked for; libctru allows 5 to 89 percent.
const SYSCORE_PERCENT: u32 = 30;

/// What a probe thread reports back.
struct Run {
    ran_on: i32,
    ticks: u64,
}

pub fn report(apt: &mut Apt) {
    let mut new_3ds = false;
    // SAFETY: a plain service call; `new_3ds` outlives it.
    let result = unsafe { ctru_sys::APT_CheckNew3DS(&mut new_3ds) };
    if ctru_sys::R_FAILED(result) {
        println!("New 3DS: couldn't ask ({result:#x})");
    } else {
        println!("New 3DS: {new_3ds}");
    }

    let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
    let app = MemRegion::Application;
    println!(
        "App memory: {:.1} MiB, {:.1} used, {:.1} free",
        mib(app.size()),
        mib(app.used()),
        mib(app.free())
    );

    match apt.set_app_cpu_time_limit(SYSCORE_PERCENT) {
        Ok(()) => println!("Core 1 share: {SYSCORE_PERCENT}%"),
        Err(e) => println!("Core 1 share refused: {e}"),
    }

    println!("Timed loop, {WORK} iterations:");
    for core in 0..4 {
        match run_on(core) {
            Some(run) => println!(
                "  core {core}: ran on {}, {:.1} ms",
                run.ran_on,
                run.ticks as f64 * 1000.0 / f64::from(ctru_sys::SYSCLOCK_ARM11)
            ),
            None => println!("  core {core}: no thread"),
        }
    }
}

/// Runs the timed loop on a thread created on `core`, or `None` if the thread couldn't be made.
fn run_on(core: i32) -> Option<Run> {
    let run = Box::into_raw(Box::new(Run {
        ran_on: -1,
        ticks: 0,
    }));
    // SAFETY: `run` stays alive until the thread is joined (or was never created) below, and only
    // the thread touches it in between.
    unsafe {
        let thread =
            ctru_sys::threadCreate(Some(timed_loop), run.cast(), STACK, PRIORITY, core, false);
        if thread.is_null() {
            drop(Box::from_raw(run));
            return None;
        }
        let _ = ctru_sys::threadJoin(thread, u64::MAX);
        ctru_sys::threadFree(thread);
        Some(*Box::from_raw(run))
    }
}

unsafe extern "C" fn timed_loop(run: *mut c_void) {
    let start = unsafe { ctru_sys::svcGetSystemTick() };
    let mut x = 0u32;
    for i in 0..WORK {
        x = black_box(x.wrapping_mul(0x9E37_79B9).wrapping_add(i));
    }
    let end = unsafe { ctru_sys::svcGetSystemTick() };
    // SAFETY: `run_on` passes a live, exclusive `Run`.
    let run = unsafe { &mut *run.cast::<Run>() };
    run.ran_on = unsafe { ctru_sys::svcGetProcessorID() };
    run.ticks = end - start;
}
