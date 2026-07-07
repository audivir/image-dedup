use crate::{
    apps::{run_dedup_app, run_sort_app},
    commands::Args,
    utils::canonicalize_dirs,
};
use clap::Parser;
use czkawka_core::common::{image::register_image_decoding_hooks, set_number_of_threads};
use log::info;
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};

mod apps;
mod commands;
mod progress;
mod sort_media;
mod ui_dedup;
mod ui_sort;
mod ui_utils;
mod utils;

fn main() {
    register_image_decoding_hooks();

    let args = Args::parse();

    // Set threads once
    set_number_of_threads(args.common_cli_items.thread_number);
    let roots = canonicalize_dirs(&args.common_cli_items.directories);

    // Setup global stop flag and Ctrl+C handler once
    let stop_flag = Arc::new(AtomicBool::new(false));
    let store_flag_cloned = stop_flag.clone();

    let _ = ctrlc::set_handler(move || {
        if store_flag_cloned.load(Ordering::SeqCst) {
            return;
        }
        info!("Got Ctrl+C signal, stopping...");
        store_flag_cloned.store(true, Ordering::SeqCst);
    });

    if args.sort_mode {
        run_sort_app(args, roots, stop_flag)
    } else {
        run_dedup_app(args, roots, stop_flag)
    }
}
