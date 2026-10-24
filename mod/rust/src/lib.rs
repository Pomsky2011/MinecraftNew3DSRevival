//! The ownership mod's logic (DESIGN.md §3.1), compiled to JVM bytecode. Java reaches it as the
//! class `n3dsr_export.n3dsr_export`; it reaches Minecraft through the bindings below, written
//! with Mojang's names, which Loom remaps to the runtime's (§3.1.1).
//!
//! For now (§14 step 2a), only enough to prove the plumbing: count the registered blocks.

#![feature(extern_types)]

#[jvm::class("net.minecraft.core.registries.BuiltInRegistries")]
impl BuiltInRegistries {
    // A static field is found by its declared type, so this must be DefaultedRegistry.
    #[jvm::static_field(name = "BLOCK")]
    pub fn block() -> *mut DefaultedRegistry {}
}

// `size` is declared on IdMap, an interface DefaultedRegistry extends; the JVM finds it there.
#[jvm::interface("net.minecraft.core.DefaultedRegistry")]
impl DefaultedRegistry {
    #[jvm::method]
    pub fn size(&self) -> i32 {}
}

/// How many blocks the game has registered.
pub fn block_count() -> i32 {
    // SAFETY: the registry is a live object for as long as the game runs.
    unsafe { (*BuiltInRegistries::block()).size() }
}
