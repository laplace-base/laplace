package org.laplace

import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * What every window of the app is made of: a webview laid out in the area the system bars leave
 * free. The webview itself is created by the Rust side, which reaches the activity through the id
 * it is started with.
 */
abstract class LaplaceActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
  }

  override fun onDestroy() {
    super.onDestroy()
    // A window closed for good takes its page with it: a webview that is only dropped goes on
    // running the page until it is collected. Not on a configuration change — `wry` re-creates the
    // webview of the same activity then.
    if (isFinishing && !isChangingConfigurations) {
      webView?.let { view ->
        (view.parent as? ViewGroup)?.removeView(view)
        view.destroy()
      }
      webView = null
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    // The window is laid out edge to edge — on Android 15+ it is anyway, whatever the app asks for
    // — and the area the system bars occupy is given back as insets below.
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // The Rust side adds the webview to this container, so padding it is what keeps the page out
    // from under the status bar, the navigation bar, a display cutout and the soft keyboard.
    ViewCompat.setOnApplyWindowInsetsListener(findViewById<View>(android.R.id.content)) { view, windowInsets ->
      val insets = windowInsets.getInsets(
        WindowInsetsCompat.Type.systemBars()
          or WindowInsetsCompat.Type.displayCutout()
          or WindowInsetsCompat.Type.ime()
      )
      view.setPadding(insets.left, insets.top, insets.right, insets.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }
}
