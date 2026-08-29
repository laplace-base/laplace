package org.laplace

/**
 * A lapp window.
 *
 * `open_lapp` (`crates/app/src/api/laplace.rs`) names this class when it builds the webview window,
 * and `tao` starts one instance of it per window. The manifest declares it as a document activity,
 * so every instance is a task of its own: the lapps are separate entries of the recents screen and
 * the launcher window in the main task goes on running behind them.
 */
class LappActivity : LaplaceActivity()
