//! What the app needs from Android itself: the data directory it unpacks the package into and the
//! windows the lapps are opened in.
//!
//! Everything here goes through JNI. The activity `tao` registers is the way into the app — the
//! class loader of a thread attached from native code knows the system classes only, so the classes
//! of the package are reached through [`app_class`], the way `tao` reaches them too.

use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use jni::objects::{JClass, JObject, JString, JValue};
use jni::{AttachGuard, JNIEnv, JavaVM};
use tauri::tao::platform::android::prelude::{AndroidContext, main_android_context};

pub use self::data::{CONFIG_FILE_NAME, DataDir, prepare_data_dir};
pub use self::window::{LAPP_ACTIVITY, move_lapp_window_to_front};

mod data;
mod window;

/// The mobile entry point runs on a thread that `tao` spawns from `WryActivity.onCreate`, before
/// the very same method registers the activity, so the context has to be waited for.
const CONTEXT_TIMEOUT: Duration = Duration::from_secs(5);
const CONTEXT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Local references held while listing a single directory or copying a single file.
const LOCAL_FRAME_CAPACITY: i32 = 32;

/// The Java VM of the process, kept so that a thread can be attached to it from anywhere.
static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// Attaches the current thread to the Java VM and hands back an activity to reach the app through.
///
/// Whichever activity comes back will do: they all live in the same process and are loaded by the
/// same class loader, and `tao` forgets the ones that are destroyed, so it is a live one.
fn attach_to_app() -> anyhow::Result<(AttachGuard<'static>, JObject<'static>)> {
    let context = wait_for_activity_context()?;
    let vm = java_vm(&context)?;

    // SAFETY: `context_jobject` is a global reference that `tao` holds for as long as the activity
    // it belongs to lives, and an activity it has forgotten is not handed out any more.
    let activity = unsafe { JObject::from_raw(context.context_jobject.cast()) };
    let env = vm
        .attach_current_thread()
        .context("Failed to attach the current thread to the Java VM")?;

    Ok((env, activity))
}

fn java_vm(context: &AndroidContext) -> anyhow::Result<&'static JavaVM> {
    if let Some(vm) = JAVA_VM.get() {
        return Ok(vm);
    }

    // SAFETY: the pointer comes from the activity registered by `tao` and is the VM of the process,
    // which lives as long as the process does.
    let vm = unsafe { JavaVM::from_raw(context.java_vm.cast()) }.context("Invalid Java VM pointer")?;

    Ok(JAVA_VM.get_or_init(|| vm))
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

/// The class of `name` in the package of the app, which only the class loader of the app knows.
fn app_class<'a>(env: &mut JNIEnv<'a>, activity: &JObject, name: &str) -> anyhow::Result<JClass<'a>> {
    let qualified = format!("{}.{name}", package_name(env, activity)?);
    let qualified = env.new_string(&qualified)?;

    let class = env.call_method(activity, "getAppClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[
        JValue::Object(&qualified),
    ]);

    let class = thrown(env, class)
        .with_context(|| format!("Failed to load the class of '{name}'"))?
        .l()?;

    Ok(JClass::from(class))
}

/// Says what the Java side has thrown, where `jni` only says that it has.
///
/// A pending exception makes every further JNI call illegal, so it is taken and cleared before it
/// is asked what it is; an error of any other kind is left as it is.
fn thrown<T>(env: &mut JNIEnv, result: jni::errors::Result<T>) -> anyhow::Result<T> {
    result.map_err(|err| match take_exception(env) {
        Some(exception) => anyhow::anyhow!("{exception}"),
        None => anyhow::Error::new(err),
    })
}

fn take_exception(env: &mut JNIEnv) -> Option<String> {
    if !env.exception_check().ok()? {
        return None;
    }

    let exception = env.exception_occurred().ok()?;
    env.exception_clear().ok()?;

    let message = env
        .call_method(&exception, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;

    Some(String::from(env.get_string(&JString::from(message)).ok()?))
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
