//! Unpacking of the `data` directory shipped inside the APK.
//!
//! An APK is a zip archive, so neither `Laplace.toml` nor the lapp frontends and backends can be
//! read from it as ordinary files. They are packed into the `assets/data` directory of the package
//! (see the `android-assets` task in `Makefile.toml`) and unpacked on every start into the
//! app-specific external storage — `/sdcard/Android/data/<package>/files/data` — after which the
//! rest of the app works with them exactly like on desktop.
//!
//! Existing files are overwritten and nothing is deleted, so whatever a lapp has written next to
//! its own files survives an update.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use anyhow::{Context as _, bail};
use jni::objects::{JObject, JObjectArray, JString, JValue};
use jni::{JNIEnv, JavaVM};
use tauri::tao::platform::android::prelude::{AndroidContext, main_android_context};

/// The assets subdirectory that is unpacked into the external storage.
const DATA_DIR: &str = "data";

/// The mobile entry point runs on a thread that `tao` spawns from `WryActivity.onCreate`, before
/// the very same method registers the activity, so the context has to be waited for.
const CONTEXT_TIMEOUT: Duration = Duration::from_secs(5);
const CONTEXT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Local references held while listing a single directory or copying a single file.
const LOCAL_FRAME_CAPACITY: i32 = 32;

/// The chunk `AssetManager.open` streams are pumped through.
const COPY_BUFFER_LEN: i32 = 64 * 1024;

/// What [`unpack_data_assets`] has written, reported once logging is initialized.
#[derive(Debug)]
pub struct UnpackedAssets {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

/// Unpacks `assets/data` from the package into the app-specific external storage.
pub fn unpack_data_assets() -> anyhow::Result<UnpackedAssets> {
    let context = wait_for_activity_context()?;

    // SAFETY: both pointers come from the activity registered by `tao` and stay valid for the
    // lifetime of the process: `java_vm` is the process-wide VM and `context_jobject` is a global
    // reference to the activity.
    let vm = unsafe { JavaVM::from_raw(context.java_vm.cast()) }.context("Invalid Java VM pointer")?;
    let activity = unsafe { JObject::from_raw(context.context_jobject.cast()) };

    let mut env = vm
        .attach_current_thread()
        .context("Failed to attach the current thread to the Java VM")?;

    let root = external_files_dir(&mut env, &activity)?;
    let assets = env
        .call_method(&activity, "getAssets", "()Landroid/content/res/AssetManager;", &[])
        .context("Failed to call Context.getAssets")?
        .l()?;

    let mut unpacked = UnpackedAssets {
        path: root.join(DATA_DIR),
        files: 0,
        bytes: 0,
    };
    copy_asset_dir(&mut env, &assets, DATA_DIR, &root, &mut unpacked)?;

    Ok(unpacked)
}

/// Waits for `tao` to register the activity the assets are read from.
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

/// The root of the unpacked data: the app-specific directory on the external storage, or the
/// internal one when the external storage is not mounted.
fn external_files_dir(env: &mut JNIEnv, activity: &JObject) -> anyhow::Result<PathBuf> {
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
        env.call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
            .context("Failed to call Context.getFilesDir")?
            .l()?
    } else {
        dir
    };

    let path = env
        .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .context("Failed to call File.getAbsolutePath")?
        .l()?;

    Ok(PathBuf::from(String::from(env.get_string(&JString::from(path))?)))
}

/// Copies the `asset_dir` assets subtree into `<root>/<asset_dir>`.
fn copy_asset_dir(
    env: &mut JNIEnv,
    assets: &JObject,
    asset_dir: &str,
    root: &Path,
    unpacked: &mut UnpackedAssets,
) -> anyhow::Result<()> {
    let target_dir = root.join(asset_dir);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("Failed to create directory '{}'", target_dir.display()))?;

    for entry in list_assets(env, assets, asset_dir)? {
        let asset_path = format!("{asset_dir}/{entry}");

        // `AssetManager` has no way to tell a file from a directory: `list` returns no entries for
        // anything that is not a directory, and directories without files are not packed at all.
        if list_assets(env, assets, &asset_path)?.is_empty() {
            unpacked.bytes += copy_asset_file(env, assets, &asset_path, &target_dir.join(&entry))?;
            unpacked.files += 1;
        } else {
            copy_asset_dir(env, assets, &asset_path, root, unpacked)?;
        }
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
