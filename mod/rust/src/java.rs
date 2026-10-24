//! Bindings to the Java classes the mod uses: the JDK's, Fabric Loader's and Minecraft's (in
//! Mojang's names; Loom remaps them, DESIGN.md §3.1.1). Each declares only what the mod calls.
//!
//! A Java exception thrown in any of these passes up through the Rust as an ordinary JVM
//! exception, for the Java caller to catch.

#![allow(clippy::new_ret_no_self)]

// The JDK.

#[jvm::class("java.lang.Object", rename_all = "camelCase")]
impl JObject {
    #[jvm::method]
    pub fn equals(&self, other: *mut JObject) -> bool {}
    #[jvm::method]
    pub fn to_string(&self) -> *mut JString {}
}

#[jvm::class("java.lang.String")]
impl JString {}

#[jvm::interface("java.util.Map", rename_all = "camelCase")]
impl Map {
    #[jvm::method]
    pub fn get(&self, key: *mut JObject) -> *mut JObject {}
}

#[jvm::class("java.util.Collections", rename_all = "camelCase")]
impl Collections {
    #[jvm::static_method]
    pub fn empty_map() -> *mut Map {}
}

#[jvm::class("java.util.Objects", rename_all = "camelCase")]
impl Objects {
    #[jvm::static_method]
    pub fn is_null(object: *mut JObject) -> bool {}
}

#[jvm::class("org.rustlang.runtime.Utf8View", rename_all = "camelCase")]
impl Utf8View {
    #[jvm::static_method]
    pub fn to_java_string(s: &str) -> *mut JString {}
    #[jvm::static_method]
    pub fn from_java_string(s: *mut JString) -> &'static str {}
}

#[jvm::class("java.lang.System", rename_all = "camelCase")]
impl System {
    #[jvm::static_method]
    pub fn get_property(key: *mut JString) -> *mut JString {}
    #[jvm::static_method]
    pub fn getenv(name: *mut JString) -> *mut JString {}
}

#[jvm::class("java.util.UUID", rename_all = "camelCase")]
impl Uuid {
    // camelCase would make this randomUuid.
    #[jvm::static_method(name = "randomUUID")]
    pub fn random_uuid() -> *mut Uuid {}
}

#[jvm::class("java.time.Instant", rename_all = "camelCase")]
impl Instant {
    #[jvm::static_method]
    pub fn now() -> *mut Instant {}
}

#[jvm::class("java.net.InetAddress")]
impl InetAddress {}

#[jvm::class("java.util.zip.ZipEntry")]
impl ZipEntry {}

#[jvm::class("java.util.zip.ZipFile", rename_all = "camelCase")]
impl ZipFile {
    #[jvm::constructor]
    pub fn new(name: *mut JString) -> *mut Self {}
    #[jvm::method]
    pub fn get_entry(&self, name: *mut JString) -> *mut ZipEntry {}
    #[jvm::method]
    pub fn close(&mut self) {}
}

#[jvm::class("java.io.File", rename_all = "camelCase")]
impl JFile {
    #[jvm::method]
    pub fn get_path(&self) -> *mut JString {}
}

// The mod's own Java.

/// Where `MinecraftMixin` keeps the folders the game was started with.
#[jvm::class("io.github.pomsky2011.n3dsr.GameFolders")]
impl GameFolders {
    #[jvm::static_field(name = "folders")]
    pub fn folders() -> *mut FolderData {}
}

// Minecraft and authlib.

#[jvm::class("net.minecraft.client.Minecraft", rename_all = "camelCase")]
impl Minecraft {
    #[jvm::static_method]
    pub fn get_instance() -> *mut Self {}
    #[jvm::method]
    pub fn get_user(&self) -> *mut User {}
    #[jvm::method]
    pub fn services(&self) -> *mut Services {}
}

#[jvm::class("net.minecraft.client.User", rename_all = "camelCase")]
impl User {
    #[jvm::method]
    pub fn get_name(&self) -> *mut JString {}
    #[jvm::method]
    pub fn get_profile_id(&self) -> *mut Uuid {}
    #[jvm::method]
    pub fn get_access_token(&self) -> *mut JString {}
}

#[jvm::class("net.minecraft.server.Services", rename_all = "camelCase")]
impl Services {
    #[jvm::method]
    pub fn session_service(&self) -> *mut MinecraftSessionService {}
}

#[jvm::interface("com.mojang.authlib.minecraft.MinecraftSessionService", rename_all = "camelCase")]
impl MinecraftSessionService {
    #[jvm::method]
    pub fn join_server(&self, profile_id: *mut Uuid, access_token: *mut JString, server_id: *mut JString) {}
    #[jvm::method]
    pub fn has_joined_server(
        &self,
        profile_name: *mut JString,
        server_id: *mut JString,
        address: *mut InetAddress,
    ) -> *mut ProfileResult {
    }
}

#[jvm::class("com.mojang.authlib.yggdrasil.ProfileResult", rename_all = "camelCase")]
impl ProfileResult {
    #[jvm::method]
    pub fn profile(&self) -> *mut GameProfile {}
}

#[jvm::class("com.mojang.authlib.GameProfile", rename_all = "camelCase")]
impl GameProfile {
    #[jvm::method]
    pub fn id(&self) -> *mut Uuid {}
}

#[jvm::class("net.minecraft.client.main.GameConfig.FolderData", rename_all = "camelCase")]
impl FolderData {
    #[jvm::field]
    pub fn asset_directory(&self) -> *mut JFile {}
    #[jvm::field]
    pub fn asset_index(&self) -> *mut JString {}
}

#[jvm::class("net.minecraft.SharedConstants", rename_all = "camelCase")]
impl SharedConstants {
    #[jvm::static_method]
    pub fn get_current_version() -> *mut WorldVersion {}
}

#[jvm::interface("net.minecraft.WorldVersion", rename_all = "camelCase")]
impl WorldVersion {
    #[jvm::method]
    pub fn id(&self) -> *mut JString {}
}

// Conversions.

/// A Rust string as a Java one.
pub fn jstr(s: &str) -> *mut JString {
    Utf8View::to_java_string(s)
}

/// Whether a Java reference is `null` (cast it with `as *mut JObject`). Not `<*mut T>::is_null`:
/// the backend lowers that as one of its emulated pointers, and a Java object isn't one.
pub fn null(object: *mut JObject) -> bool {
    Objects::is_null(object)
}

/// Java's `null`, for passing where Java takes one (cast it to the parameter's type). Not
/// `ptr::null_mut()`: the backend lowers that to a `Pointer.nullPointer` that doesn't exist for
/// Java classes. An empty map's `get` always returns `null`.
pub fn java_null() -> *mut JObject {
    unsafe { (*Collections::empty_map()).get(jstr("") as *mut JObject) }
}

/// A Java string as a Rust one; `None` for Java's `null`.
pub fn rstr(s: *mut JString) -> Option<String> {
    (!null(s as *mut JObject)).then(|| Utf8View::from_java_string(s).to_owned())
}

/// Any Java object's `toString()` (cast it with `as *mut JObject`).
pub fn display(object: *mut JObject) -> String {
    rstr(unsafe { (*object).to_string() }).unwrap_or_default()
}

/// A system property, or `None` if it isn't set.
pub fn property(key: &str) -> Option<String> {
    rstr(System::get_property(jstr(key)))
}

/// An environment variable, or `None` if it isn't set.
pub fn env(name: &str) -> Option<String> {
    rstr(System::getenv(jstr(name)))
}
