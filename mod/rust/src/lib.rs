//! The ownership mod's logic (DESIGN.md §3.1), compiled to JVM bytecode. Java reaches it as the
//! class `n3dsr_export.n3dsr_export`; it reaches Minecraft through `java`'s bindings, written in
//! Mojang's names, which Loom remaps to the runtime's (§3.1.1).

#![feature(extern_types)]
// `&str` crosses to Java as the runtime's Utf8View, which rustc's FFI lint doesn't know.
#![allow(improper_ctypes)]

mod java;
mod proof;

use java::{JString, jstr};

/// What a check came to, as Java sees it: `Outcome.Proven` or `Outcome.Refused`, each with a
/// `value`, the proof's path or the reason.
pub enum Outcome {
    Proven(*mut JString),
    Refused(*mut JString),
}

/// Whether the game was launched to check ownership and quit (`-Dn3dsr.export=true`).
pub fn launch_requested() -> bool {
    java::property("n3dsr.export").as_deref() == Some("true")
}

/// Checks that the signed-in account owns the game, then writes the proof (DESIGN.md §2.2,
/// §3.1). Blocks on the network: never call it on the render thread. Exceptions from authlib (an
/// offline session, Mojang unreachable) pass through to the caller.
pub fn check() -> Outcome {
    match proof::check_and_write() {
        Ok(path) => Outcome::Proven(jstr(&path)),
        Err(reason) => Outcome::Refused(jstr(&reason)),
    }
}

/// The command that builds the 3DS app from a proof.
pub fn command_for(proof: *mut JString) -> *mut JString {
    let proof = java::rstr(proof).unwrap_or_default();
    jstr(&format!("n3dsr-build \"{proof}\""))
}
