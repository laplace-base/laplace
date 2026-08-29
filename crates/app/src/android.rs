//! The data directory of the Android package and the assets it is filled from.
//!
//! The lapps live in a directory of the shared storage — `Documents/Laplace` — so that it outlives
//! the app, is visible over MTP and can be written to from a file manager. Reaching it by path
//! needs the all-files access, which is a special permission the user grants in a settings screen;
//! until they do, the app falls back to its own sandbox directory, which the system wipes when the
//! package is uninstalled.
//!
//! The directory is filled from the `data` assets of the APK (see the `android-assets` task in
//! `Makefile.toml`). A file missing from it is always unpacked; an existing one is overwritten only
//! when the package itself has changed since the last unpacking, and the settings file is left
//! alone entirely — once it is there, it belongs to the user.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use anyhow::{Context as _, bail};
use jni::objects::{JObject, JObjectArray, JString, JValue};
use jni::{JNIEnv, JavaVM};
use tauri::tao::platform::android::prelude::{AndroidContext, main_android_context};

/// The settings file, read by the host and editable by the user.
pub const CONFIG_FILE_NAME: &str = "Laplace.toml";

/// The assets subdirectory whose contents make up the data directory.
const DATA_ASSET_DIR: &str = "data";
/// The data directory inside the shared `Documents` one.
const SHARED_DIR_NAME: &str = "Laplace";
/// Holds the package stamp the data was unpacked from, so that a reinstall refreshes the files and
/// an ordinary restart leaves them as they are.
const STAMP_FILE_NAME: &str = ".unpacked";

/// The mobile entry point runs on a thread that `tao` spawns from `WryActivity.onCreate`, before
/// the very same method registers the activity, so the context has to be waited for.
const CONTEXT_TIMEOUT: Duration = Duration::from_secs(5);
const CONTEXT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Local references held while listing a single directory or copying a single file.
const LOCAL_FRAME_CAPACITY: i32 = 32;

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
    let context = wait_for_activity_context()?;

    // SAFETY: both pointers come from the activity registered by `tao` and stay valid for the
    // lifetime of the process: `java_vm` is the process-wide VM and `context_jobject` is a global
    // reference to the activity.
    let vm = unsafe { JavaVM::from_raw(context.java_vm.cast()) }.context("Invalid Java VM pointer")?;
    let activity = unsafe { JObject::from_raw(context.context_jobject.cast()) };

    let mut env = vm
        .attach_current_thread()
        .context("Failed to attach the current thread to the Java VM")?;

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
    // A file the user has changed is kept until the package itself changes.
    let refresh = read_stamp(&path) != Some(stamp);

    let assets = env
        .call_method(&activity, "getAssets", "()Landroid/content/res/AssetManager;", &[])
        .context("Failed to call Context.getAssets")?
        .l()?;

    let mut unpacked = Unpacked::default();
    unpack_asset_dir(&mut env, &assets, DATA_ASSET_DIR, &path, refresh, &mut unpacked)?;

    if refresh {
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

/// Waits for `tao` to register the activity everything else is reached through.
fn wait_for_activity_context() -> anyhow::Result<AndroidContext> {
    let deadline = Instant::now() + CONTEXT_TIMEOUT;

    loop {
        if let Some(context) = main_android_context() {
            return Ok(context);
        }
        if Instant::now() >= deadline {
            bail!("The activity has not been created in {CONTEXT_TIMEOUT:?}");
        }
        thread::sleep(CONTEXT_POLL_INTERVAL);
    }
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

/// Unpacks the contents of the `asset_dir` assets subtree into `target_dir`.
///
/// A file that is not there yet is always written; an existing one is replaced only when `refresh`
/// is set, and the settings file is never replaced at all.
fn unpack_asset_dir(
    env: &mut JNIEnv,
    assets: &JObject,
    asset_dir: &str,
    target_dir: &Path,
    refresh: bool,
    unpacked: &mut Unpacked,
) -> anyhow::Result<()> {
    fs::create_dir_all(target_dir).with_context(|| format!("Failed to create directory '{}'", target_dir.display()))?;

    for entry in list_assets(env, assets, asset_dir)? {
        let asset_path = format!("{asset_dir}/{entry}");
        let target_path = target_dir.join(&entry);

        // `AssetManager` has no way to tell a file from a directory: `list` returns no entries for
        // anything that is not a directory, and directories without files are not packed at all.
        if !list_assets(env, assets, &asset_path)?.is_empty() {
            unpack_asset_dir(env, assets, &asset_path, &target_path, refresh, unpacked)?;
            continue;
        }

        if target_path.exists() && (!refresh || entry == CONFIG_FILE_NAME) {
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

fn sdk_int(env: &mut JNIEnv) -> anyhow::Result<i32> {
    Ok(env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
        .context("Failed to read Build.VERSION.SDK_INT")?
        .i()?)
}

fn package_name(env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<String> {
    let name = env
        .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])
        .context("Failed to call Context.getPackageName")?
        .l()?;

    Ok(String::from(env.get_string(&JString::from(name))?))
}

/// `java.io.File.getAbsolutePath`.
fn absolute_path(env: &mut JNIEnv, file: &JObject) -> anyhow::Result<PathBuf> {
    let path = env
        .call_method(file, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .context("Failed to call File.getAbsolutePath")?
        .l()?;

    Ok(PathBuf::from(String::from(env.get_string(&JString::from(path))?)))
}
