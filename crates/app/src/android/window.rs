//! The windows the lapps are opened in.
//!
//! A window is an activity on Android, and every lapp window is an instance of [`LAPP_ACTIVITY`],
//! declared in `AndroidManifest.xml` as a document activity: each instance is a task of its own, so
//! the lapps are separate entries of the recents screen and the launcher window goes on running
//! behind them.

use anyhow::Context as _;
use jni::objects::JValue;

/// The activity class of a lapp window, relative to the package — the form `tao` starts one by.
pub const LAPP_ACTIVITY: &str = "LappActivity";

/// Brings the task of the window showing the lapp `name` to the front, telling whether there was
/// one to bring.
///
/// The windows are tasks of their own, and `tao` cannot raise a task: `Window::set_focus` is a no-op
/// on Android. The activity knows how, though — it is the lapp windows themselves that keep track of
/// which lapp each of them shows, by the name `wry` gives the webview, which is the window label.
pub fn move_lapp_window_to_front(name: &str) -> anyhow::Result<bool> {
    let (mut env, activity) = super::attach_to_app()?;
    let class = super::app_class(&mut env, &activity, LAPP_ACTIVITY)?;
    let name = env.new_string(name)?;

    let raised = env.call_static_method(&class, "moveToFront", "(Ljava/lang/String;)Z", &[JValue::Object(&name)]);

    Ok(super::thrown(&mut env, raised)
        .context("Failed to call LappActivity.moveToFront")?
        .z()?)
}
