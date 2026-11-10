#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use time_block_planner::storage::{Store, resolve_dir};
use time_block_planner::ui::PlannerApp;

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let mut cli_dir = None;
    while let Some(arg) = args.next() {
        if arg == "--dir" {
            cli_dir = args.next().map(PathBuf::from);
        }
    }
    let store = Store { dir: resolve_dir(cli_dir) };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("time-block-planner")
            .with_inner_size([520.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "time-block-planner",
        options,
        Box::new(|cc| Ok(Box::new(PlannerApp::new(cc, store)))),
    )
}
