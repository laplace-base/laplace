package org.laplace

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import android.util.Log
import android.webkit.WebView

/**
 * A lapp window.
 *
 * `open_lapp` (`crates/app/src/api/laplace.rs`) names this class when it builds the webview window,
 * and `tao` starts one instance of it per window. The manifest declares it as a document activity,
 * so every instance is a task of its own: the lapps are separate entries of the recents screen and
 * the launcher window in the main task goes on running behind them. Nothing here is per lapp — the
 * lapps are added and removed by the user, so one class serves them all, however many are open.
 */
class LappActivity : LaplaceActivity() {
  companion object {
    /** What the Rust side logs under, so that both halves of the app read as one log. */
    private const val LOG_TAG = "Laplace"

    /** The open windows by the name of the lapp each one shows. */
    private val open = HashMap<String, LappActivity>()

    /**
     * Brings the window of the lapp `name` to the front, telling whether there was one to bring.
     *
     * Called through JNI by `android::window::move_lapp_window_to_front` when a lapp that is
     * already open is opened again. The window is a task of its own, so it is raised as one:
     * nothing else brings a task back from the recents screen.
     */
    @JvmStatic
    fun moveToFront(name: String): Boolean {
      val activity = synchronized(open) { open[name] } ?: return false
      return activity.moveTaskToFront()
    }
  }

  private var lapp: String? = null

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    // `wry` names the webview after the label of the window, which is the name of the lapp.
    val name = (webView as? RustWebView)?.id ?: return
    lapp = name
    synchronized(open) { open[name] = this }
  }

  override fun onDestroy() {
    super.onDestroy()
    val name = lapp ?: return
    // A window that has replaced this one in the meantime is the one that stays registered.
    synchronized(open) { if (open[name] === this) open.remove(name) }
  }

  /**
   * Raises the task this window sits in.
   *
   * Everything here is a call into the system server, and it throws for a task that is on its way
   * out — the tasks of the app are asked about one by one so that such a one is skipped instead of
   * ending the search, and a failure is logged rather than thrown back into the JNI caller, which
   * would see nothing but "an exception was thrown".
   */
  private fun moveTaskToFront(): Boolean {
    val manager = getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager

    for (task in manager.appTasks) {
      val id = try {
        val info = task.taskInfo
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
          info.taskId
        } else {
          @Suppress("DEPRECATION")
          info.id
        }
      } catch (error: Exception) {
        Log.w(LOG_TAG, "Failed to read the info of a task of the app", error)
        continue
      }

      if (id != taskId) {
        continue
      }

      return try {
        task.moveToFront()
        true
      } catch (error: Exception) {
        Log.w(LOG_TAG, "Failed to move the task of lapp '$lapp' to the front", error)
        false
      }
    }

    Log.w(LOG_TAG, "The task of lapp '$lapp' is not among the tasks of the app")
    return false
  }
}
