//! The data directory of the package and the assets it is filled from.
//!
//! The lapps live in a directory of the shared storage — `Documents/Laplace` — so that it outlives
//! the app, is visible over MTP and can be written to from a file manager. Reaching it by path
//! needs the all-files access, which is a special permission the user grants in a settings screen;
//! until they do, the app falls back to its own sandbox directory, which the system wipes when the
//! package is uninstalled.
//!
//! The directory is filled from the `data` assets of the APK (see the `android-assets` task in
//! `Makefile.toml`), and only on the first start of a newly installed or updated package: a file
//! that is missing is written, and one that is already there is replaced only if
//! `platform.mobile.overwrite_existing_assets` allows it. Every other start unpacks nothing and
//! runs on what is in the data directory, so a file the user has deleted does not come back and a
//! file they have changed stays changed. The settings file is the single exception: it is put back
//! whenever it is missing, because nothing can start without it, and it is never replaced.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context as _, bail};
use jni::JNIEnv;
use jni::objects::{JObject, JObjectArray, JString, JValue};
use laplace_settings::LaplaceSettings;

use super::{LOCAL_FRAME_CAPACITY, package_name, sdk_int};

/// The settings file, read by the host and editable by the user.
pub const CONFIG_FILE_NAME: &str = "Laplace.toml";

/// The assets subdirectory whose contents make up the data directory.
const DATA_ASSET_DIR: &str = "data";
/// The data directory inside the shared `Documents` one.
const SHARED_DIR_NAME: &str = "Laplace";
/// Holds the package stamp the data was unpacked from, so that a reinstall refreshes the files and
/// an ordinary restart leaves them as they are.
const STAMP_FILE_NAME: &str = ".unpacked";

/// The chunk `AssetManager.open` streams are pumped through.
const COPY_BUFFER_LEN: i32 = 64 * 1024;

/// Android 11, where `MANAGE_EXTERNAL_STORAGE` replaced the storage runtime permissions.
const ANDROID_11: i32 = 30;
const PERMISSION_GRANTED: i32 = 0;
const WRITE_EXTERNAL_STORAGE: &str = "android.permission.WRITE_EXTERNAL_STORAGE";
const ALL_FILES_ACCESS_SETTINGS: &str = "android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION";
const APP_DETAILS_SETTINGS: &str = "android.settings.APPLICATION_DETAILS_SETTINGS";

/// The prepared data directory, reported once logging is initialized.
#[derive(Debug)]
pub struct DataDir {
    /// The path of the data directory.
    pub path: PathBuf,
    /// Whether [`Self::path`] is the shared directory rather than the sandbox fallback.
    pub shared: bool,
    pub unpacked_files: usize,
    pub unpacked_bytes: u64,
    /// What went wrong without being fatal — there is no logger yet while this is filled in.
    pub warnings: Vec<String>,
}

impl DataDir {
    pub fn config_path(&self) -> PathBuf {
        self.path.join(CONFIG_FILE_NAME)
    }
}

/// Picks the data directory and unpacks whatever of the packaged data is missing from it.
pub fn prepare_data_dir() -> anyhow::Result<DataDir> {
    let (mut env, activity) = super::attach_to_app()?;

    let mut warnings = Vec::new();
    let shared = has_full_storage_access(&mut env, &activity)?;
    let path = if shared {
        shared_data_dir(&mut env)?
    } else {
        // Let the user grant the access; the app picks the shared directory up on the next start.
        open_storage_settings(&mut env, &activity)?;
        warnings.push(
            "No access to the shared storage: the lapps are kept in the sandbox directory, which is \
             wiped when the app is uninstalled and is not writable from outside. Grant the file \
             access in the settings screen that has just been opened and restart the app."
                .to_string(),
        );
        sandbox_data_dir(&mut env, &activity)?
    };

    let stamp = package_stamp(&mut env, &activity)?;
    // The one start that unpacks anything: from here on the data directory is the user's.
    let package_changed = read_stamp(&path) != Some(stamp);

    let assets = env
        .call_method(&activity, "getAssets", "()Landroid/content/res/AssetManager;", &[])
        .context("Failed to call Context.getAssets")?
        .l()?;

    let mut unpacked = Unpacked::default();
    // The settings say how the rest of the data is to be unpacked, so they come first — and they
    // are the one file the app cannot start without, so they are restored whenever they are gone.
    let settings = unpack_settings(&mut env, &assets, &path, &mut unpacked).unwrap_or_else(|err| {
        warnings.push(format!(
            "Failed to read the settings of the data directory: {err:#}. The unpacking falls back to \
             its defaults."
        ));
        LaplaceSettings::default()
    });

    // An ordinary start unpacks nothing at all: whatever the user has deleted stays deleted, and
    // whatever they have changed stays changed, until the package itself is installed anew.
    if package_changed {
        let overwrite = settings.platform.mobile.overwrite_existing_assets;
        unpack_asset_dir(&mut env, &assets, DATA_ASSET_DIR, &path, overwrite, &mut unpacked)?;
        write_stamp(&path, stamp)?;
    }

    // Files written by path are not indexed on their own, and without the index they are invisible
    // over MTP and to the apps that go through `MediaStore`.
    if shared
        && !unpacked.paths.is_empty()
        && let Err(err) = scan_files(&mut env, &activity, &unpacked.paths)
    {
        warnings.push(format!(
            "Failed to hand the unpacked files to the media scanner: {err:#}"
        ));
    }

    Ok(DataDir {
        path,
        shared,
        unpacked_files: unpacked.files,
        unpacked_bytes: unpacked.bytes,
        warnings,
    })
}

/// Whether the app may read and write the shared storage by path.
fn has_full_storage_access(env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<bool> {
    if sdk_int(env)? >= ANDROID_11 {
        // `MANAGE_EXTERNAL_STORAGE` is a special access: it is never granted by a request dialog.
        Ok(env
            .call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])
            .context("Failed to call Environment.isExternalStorageManager")?
            .z()?)
    } else {
        let permission = env.new_string(WRITE_EXTERNAL_STORAGE)?;
        let granted = env
            .call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[
                JValue::Object(&permission),
            ])
            .context("Failed to call Context.checkSelfPermission")?
            .i()?;

        Ok(granted == PERMISSION_GRANTED)
    }
}

/// Opens the settings screen the storage access is granted in.
fn open_storage_settings(env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<()> {
    let action = if sdk_int(env)? >= ANDROID_11 {
        ALL_FILES_ACCESS_SETTINGS
    } else {
        APP_DETAILS_SETTINGS
    };

    let package = package_name(env, activity)?;
    let uri = env.new_string(format!("package:{package}"))?;
    let uri = env
        .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[
            JValue::Object(&uri),
        ])
        .context("Failed to call Uri.parse")?
        .l()?;

    let action = env.new_string(action)?;
    let intent = env
        .new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[
            JValue::Object(&action),
            JValue::Object(&uri),
        ])
        .context("Failed to create the settings intent")?;

    env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[
        JValue::Object(&intent),
    ])
    .context("Failed to open the storage settings")?;

    Ok(())
}

/// `Documents/Laplace` on the shared storage: it outlives the package and is writable from outside.
fn shared_data_dir(env: &mut JNIEnv) -> anyhow::Result<PathBuf> {
    let documents = env
        .get_static_field("android/os/Environment", "DIRECTORY_DOCUMENTS", "Ljava/lang/String;")
        .context("Failed to read Environment.DIRECTORY_DOCUMENTS")?
        .l()?;

    let dir = env
        .call_static_method(
            "android/os/Environment",
            "getExternalStoragePublicDirectory",
            "(Ljava/lang/String;)Ljava/io/File;",
            &[JValue::Object(&documents)],
        )
        .context("Failed to call Environment.getExternalStoragePublicDirectory")?
        .l()?;

    Ok(absolute_path(env, &dir)?.join(SHARED_DIR_NAME))
}

/// The fallback: the app-specific directory, which needs no access of any kind.
fn sandbox_data_dir(env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<PathBuf> {
    let no_type = JObject::null();
    let dir = env
        .call_method(
            activity,
            "getExternalFilesDir",
            "(Ljava/lang/String;)Ljava/io/File;",
            &[JValue::Object(&no_type)],
        )
        .context("Failed to call Context.getExternalFilesDir")?
        .l()?;

    let dir = if dir.is_null() {
        // The external storage is not mounted; the internal one is always available.
        env.call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
            .context("Failed to call Context.getFilesDir")?
            .l()?
    } else {
        dir
    };

    Ok(absolute_path(env, &dir)?.join(DATA_ASSET_DIR))
}

/// The moment the package was installed or updated, which is what makes the data stale.
fn package_stamp(env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<i64> {
    let manager = env
        .call_method(
            activity,
            "getPackageManager",
            "()Landroid/content/pm/PackageManager;",
            &[],
        )
        .context("Failed to call Context.getPackageManager")?
        .l()?;

    let package = package_name(env, activity)?;
    let package = env.new_string(package)?;
    let info = env
        .call_method(
            &manager,
            "getPackageInfo",
            "(Ljava/lang/String;I)Landroid/content/pm/PackageInfo;",
            &[JValue::Object(&package), JValue::Int(0)],
        )
        .context("Failed to call PackageManager.getPackageInfo")?
        .l()?;

    Ok(env
        .get_field(&info, "lastUpdateTime", "J")
        .context("Failed to read PackageInfo.lastUpdateTime")?
        .j()?)
}

fn read_stamp(data_dir: &Path) -> Option<i64> {
    fs::read_to_string(data_dir.join(STAMP_FILE_NAME))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn write_stamp(data_dir: &Path, stamp: i64) -> anyhow::Result<()> {
    let path = data_dir.join(STAMP_FILE_NAME);
    fs::write(&path, stamp.to_string()).with_context(|| format!("Failed to write file '{}'", path.display()))
}

/// What [`unpack_asset_dir`] has written.
#[derive(Default)]
struct Unpacked {
    files: usize,
    bytes: u64,
    paths: Vec<String>,
}

/// Puts the settings file in place, if it is not there yet, and reads it.
///
/// It is what the unpacking of everything else is governed by, so it cannot wait for its turn in
/// [`unpack_asset_dir`] — and on the very first start it exists as a package asset only. Unlike
/// everything else it is restored on any start, not just the one after an installation: the app
/// has nothing to read its settings from otherwise.
fn unpack_settings(
    env: &mut JNIEnv,
    assets: &JObject,
    target_dir: &Path,
    unpacked: &mut Unpacked,
) -> anyhow::Result<LaplaceSettings> {
    let target_path = target_dir.join(CONFIG_FILE_NAME);
    let asset_path = format!("{DATA_ASSET_DIR}/{CONFIG_FILE_NAME}");

    if !target_path.exists() {
        if !list_assets(env, assets, DATA_ASSET_DIR)?
            .iter()
            .any(|entry| entry == CONFIG_FILE_NAME)
        {
            bail!(
                "Neither '{}' nor the asset '{asset_path}' exists",
                target_path.display()
            );
        }

        fs::create_dir_all(target_dir)
            .with_context(|| format!("Failed to create directory '{}'", target_dir.display()))?;

        unpacked.bytes += copy_asset_file(env, assets, &asset_path, &target_path)?;
        unpacked.files += 1;
        unpacked.paths.push(target_path.to_string_lossy().into_owned());
    }

    LaplaceSettings::new(&target_path).with_context(|| format!("Failed to read file '{}'", target_path.display()))
}

/// Unpacks the contents of the `asset_dir` assets subtree into `target_dir`.
///
/// Called on the first start of a newly installed or updated package alone. A file that is not
/// there yet is written; an existing one is replaced only when `overwrite` is set. The settings
/// file is never replaced at all — [`unpack_settings`] has already put it there and from then on
/// it belongs to the user.
fn unpack_asset_dir(
    env: &mut JNIEnv,
    assets: &JObject,
    asset_dir: &str,
    target_dir: &Path,
    overwrite: bool,
    unpacked: &mut Unpacked,
) -> anyhow::Result<()> {
    fs::create_dir_all(target_dir).with_context(|| format!("Failed to create directory '{}'", target_dir.display()))?;

    for entry in list_assets(env, assets, asset_dir)? {
        let asset_path = format!("{asset_dir}/{entry}");
        let target_path = target_dir.join(&entry);

        // `AssetManager` has no way to tell a file from a directory: `list` returns no entries for
        // anything that is not a directory, and directories without files are not packed at all.
        if !list_assets(env, assets, &asset_path)?.is_empty() {
            unpack_asset_dir(env, assets, &asset_path, &target_path, overwrite, unpacked)?;
            continue;
        }

        if target_path.exists() && (!overwrite || entry == CONFIG_FILE_NAME) {
            continue;
        }

        unpacked.bytes += copy_asset_file(env, assets, &asset_path, &target_path)?;
        unpacked.files += 1;
        unpacked.paths.push(target_path.to_string_lossy().into_owned());
    }

    Ok(())
}

/// The names of the assets directly inside `asset_dir`, empty if it is a file.
fn list_assets(env: &mut JNIEnv, assets: &JObject, asset_dir: &str) -> anyhow::Result<Vec<String>> {
    env.with_local_frame(LOCAL_FRAME_CAPACITY, |env| -> jni::errors::Result<_> {
        let dir = env.new_string(asset_dir)?;
        let list = env
            .call_method(assets, "list", "(Ljava/lang/String;)[Ljava/lang/String;", &[
                JValue::Object(&dir),
            ])?
            .l()?;

        let list = JObjectArray::from(list);
        let length = env.get_array_length(&list)?;
        let mut entries = Vec::with_capacity(length as usize);

        for index in 0..length {
            let entry = env.get_object_array_element(&list, index)?;
            entries.push(String::from(env.get_string(&JString::from(entry))?));
        }

        Ok(entries)
    })
    .with_context(|| format!("Failed to list the assets of '{asset_dir}'"))
}

/// Copies a single asset, returning the number of bytes written.
fn copy_asset_file(env: &mut JNIEnv, assets: &JObject, asset_path: &str, target_path: &Path) -> anyhow::Result<u64> {
    let file =
        fs::File::create(target_path).with_context(|| format!("Failed to create file '{}'", target_path.display()))?;
    let mut file = io::BufWriter::new(file);
    let mut copied = 0;

    env.with_local_frame(LOCAL_FRAME_CAPACITY, |env| -> anyhow::Result<()> {
        let path = env.new_string(asset_path)?;
        let stream = env
            .call_method(assets, "open", "(Ljava/lang/String;)Ljava/io/InputStream;", &[
                JValue::Object(&path),
            ])?
            .l()?;
        let buffer = env.new_byte_array(COPY_BUFFER_LEN)?;

        // `InputStream.readAllBytes` needs API 33, so the stream is pumped chunk by chunk.
        loop {
            let read = env
                .call_method(&stream, "read", "([B)I", &[JValue::Object(&buffer)])?
                .i()?;
            if read < 0 {
                break;
            }

            let chunk = env.convert_byte_array(&buffer)?;
            file.write_all(&chunk[..read as usize])?;
            copied += read as u64;
        }
        env.call_method(&stream, "close", "()V", &[])?;

        Ok(())
    })
    .with_context(|| format!("Failed to copy the asset '{asset_path}' to '{}'", target_path.display()))?;

    file.flush()
        .with_context(|| format!("Failed to write file '{}'", target_path.display()))?;

    Ok(copied)
}

/// Adds the written files to `MediaStore`, which is what makes them visible over MTP.
fn scan_files(env: &mut JNIEnv, activity: &JObject, paths: &[String]) -> anyhow::Result<()> {
    env.with_local_frame(LOCAL_FRAME_CAPACITY, |env| -> jni::errors::Result<()> {
        let null = JObject::null();
        let array = env.new_object_array(paths.len() as i32, "java/lang/String", &null)?;

        for (index, path) in paths.iter().enumerate() {
            let path = env.new_string(path)?;
            env.set_object_array_element(&array, index as i32, &path)?;
        }

        env.call_static_method(
            "android/media/MediaScannerConnection",
            "scanFile",
            "(Landroid/content/Context;[Ljava/lang/String;[Ljava/lang/String;\
             Landroid/media/MediaScannerConnection$OnScanCompletedListener;)V",
            &[
                JValue::Object(activity),
                JValue::Object(&array),
                JValue::Object(&null),
                JValue::Object(&null),
            ],
        )?;

        Ok(())
    })
    .context("Failed to call MediaScannerConnection.scanFile")
}

/// `java.io.File.getAbsolutePath`.
fn absolute_path(env: &mut JNIEnv, file: &JObject) -> anyhow::Result<PathBuf> {
    let path = env
        .call_method(file, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .context("Failed to call File.getAbsolutePath")?
        .l()?;

    Ok(PathBuf::from(String::from(env.get_string(&JString::from(path))?)))
}
