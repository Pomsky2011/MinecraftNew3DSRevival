//! The ownership check and the proof it writes (DESIGN.md §2.2, §3.1).
//!
//! The check is the one online-mode servers make: the client tells Mojang's session server it is
//! joining a server ID (`joinServer`), and the "server", here us, asks whether that account did
//! (`hasJoinedServer`). Both are vanilla's own calls; the access token goes only to `joinServer`,
//! as when joining a server, and is never stored or logged.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha1_smol::Sha1;

use crate::java::{self, GameFolders, Instant, JObject, Minecraft, SharedConstants, Uuid, ZipFile};

/// Bumped when the proof's shape changes, so `n3dsr-build` can tell. 2 added `game_directory`.
const FORMAT: u32 = 2;

/// The client jar's one class Mojang never obfuscates: the game's entrypoint.
const CLIENT_MAIN: &str = "net/minecraft/client/main/Main.class";

#[derive(Serialize)]
struct Proof {
    format: u32,
    release: String,
    account: Account,
    checked_at: String,
    /// Where `options.txt` and `resourcepacks/` are, for the resource packs (DESIGN.md §3.5).
    game_directory: String,
    client_jar: File,
    assets: Assets,
}

#[derive(Serialize)]
struct Account {
    name: String,
    uuid: String,
}

#[derive(Serialize)]
struct File {
    path: String,
    sha1: String,
}

#[derive(Serialize)]
struct Assets {
    /// The launcher's asset store: `indexes/<index>.json` and `objects/<hash prefix>/<hash>`.
    directory: String,
    index: String,
    index_sha1: String,
}

/// Runs the check and writes the proof, returning its path, or why there's no proof.
pub fn check_and_write() -> Result<String, String> {
    let (minecraft, release) = unsafe {
        let minecraft = &*Minecraft::get_instance();
        let release = java::rstr((*SharedConstants::get_current_version()).id()).unwrap_or_default();
        (minecraft, release)
    };
    let (name, uuid) = unsafe { verify_ownership(minecraft)? };

    let client_jar = find_client_jar().ok_or("couldn't find the client jar on the classpath")?;
    let folders = game_folders().ok_or("the game's folders weren't recorded")?;
    let (game_dir, assets_dir, index) = folders;
    let index_file = Path::new(&assets_dir).join("indexes").join(format!("{index}.json"));

    let proof = Proof {
        format: FORMAT,
        release: release.clone(),
        account: Account { name, uuid },
        checked_at: java::display(Instant::now() as *mut JObject),
        game_directory: game_dir,
        client_jar: File {
            sha1: sha1_of(Path::new(&client_jar))?,
            path: client_jar,
        },
        assets: Assets {
            index_sha1: sha1_of(&index_file)?,
            directory: assets_dir,
            index,
        },
    };

    let path = proofs_dir()?.join(format!("{release}.json"));
    write_atomically(&path, &serde_json::to_vec_pretty(&proof).unwrap())?;
    Ok(path.display().to_string())
}

/// Asks Mojang, as an online-mode server would, whether the signed-in account owns the game.
/// Returns its name and UUID.
unsafe fn verify_ownership(minecraft: &Minecraft) -> Result<(String, String), String> {
    unsafe {
        let user = &*minecraft.get_user();
        let sessions = &*(*minecraft.services()).session_service();
        let profile_id = user.get_profile_id();

        // A server ID only we know, so a stale "joined" record from elsewhere can't answer for us.
        let nonce = java::display(Uuid::random_uuid() as *mut JObject);
        let server_id = Sha1::from(nonce).digest().to_string();
        let server_id = java::jstr(&server_id);

        sessions.join_server(profile_id, user.get_access_token(), server_id);
        // No address: like an online-mode server without prevent-proxy-connections.
        let no_address = java::java_null() as *mut java::InetAddress;
        let result = sessions.has_joined_server(user.get_name(), server_id, no_address);
        if java::null(result as *mut JObject) {
            return Err("Mojang didn't confirm this session; is the game signed in?".into());
        }
        let confirmed = (*result).profile();
        if !(*((*confirmed).id() as *mut JObject)).equals(profile_id as *mut JObject) {
            return Err("Mojang confirmed a different account than the one signed in".into());
        }
        Ok((java::rstr(user.get_name()).unwrap_or_default(), java::display(profile_id as *mut JObject)))
    }
}

/// The jar on the classpath holding the client's entrypoint. Fabric's runtime-remapped copy of
/// the game isn't on the classpath; the launcher's original is.
fn find_client_jar() -> Option<String> {
    let classpath = java::property("java.class.path")?;
    let separator = java::property("path.separator").unwrap_or_else(|| ":".into());
    classpath
        .split(separator.as_str())
        .filter(|entry| entry.ends_with(".jar"))
        .find(|jar| unsafe {
            let zip = ZipFile::new(java::jstr(jar));
            let found = !java::null((*zip).get_entry(java::jstr(CLIENT_MAIN)) as *mut JObject);
            (*zip).close();
            found
        })
        .map(str::to_owned)
}

/// The game directory, asset directory and asset index the game was started with (`--gameDir`,
/// `--assetsDir`, `--assetIndex`), as `MinecraftMixin` recorded them.
fn game_folders() -> Option<(String, String, String)> {
    unsafe {
        let folders = GameFolders::folders();
        if java::null(folders as *mut JObject) {
            return None;
        }
        let game = java::rstr((*(*folders).game_directory()).get_path())?;
        let assets = java::rstr((*(*folders).asset_directory()).get_path())?;
        Some((game, assets, java::rstr((*folders).asset_index())?))
    }
}

/// `<config dir>/n3dsr/proofs`, where `n3dsr-build` looks (the `dirs` crate's config directory).
fn proofs_dir() -> Result<PathBuf, String> {
    let os = java::property("os.name").unwrap_or_default().to_lowercase();
    let home = java::property("user.home").ok_or("no home directory")?;
    let config = if os.contains("win") {
        java::env("APPDATA").ok_or("APPDATA isn't set")?.into()
    } else if os.contains("mac") {
        Path::new(&home).join("Library/Application Support")
    } else {
        java::env("XDG_CONFIG_HOME")
            .filter(|dir| Path::new(dir).is_absolute())
            .map_or_else(|| Path::new(&home).join(".config"), PathBuf::from)
    };
    Ok(config.join("n3dsr").join("proofs"))
}

/// Slow on the JVM backend (some 7 s for a 30 MB jar: every byte goes through its emulated
/// memory), so only ever on a background thread.
fn sha1_of(file: &Path) -> Result<String, String> {
    let bytes = fs::read(file).map_err(|e| format!("couldn't read {}: {e}", file.display()))?;
    Ok(Sha1::from(bytes).digest().to_string())
}

/// Writes beside the target, then renames, so a half-written proof never sits where one is read.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("couldn't write {}: {e}", path.display());
    fs::create_dir_all(path.parent().unwrap()).map_err(fail)?;
    let partial = path.with_extension("json.partial");
    fs::write(&partial, bytes).map_err(fail)?;
    fs::rename(&partial, path).map_err(fail)
}
