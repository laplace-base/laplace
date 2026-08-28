// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use laplace_settings::LaplaceSettings;

mod cli;

fn main() {
    let opts: cli::Opts = cli::Opts::parse();
    let settings = LaplaceSettings::new(&opts.config).expect("Settings should be configured");

    laplace_app_lib::logging::init(&settings.log).expect("Logger should be initialized");
    laplace_app_lib::run_desktop(settings);
}
