package org.laplace

import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
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
