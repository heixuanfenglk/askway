// Windows：以 GUI 子系统运行，不弹出控制台黑窗
#![windows_subsystem = "windows"]

mod api;
mod app;
mod attachments;
mod claude_cli;
mod md_table;
mod models;
mod storage;

use app::AskwayApp;
use eframe::egui;
use std::sync::Arc;

fn load_app_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon_256.png");
    let image = image::load_from_memory(bytes)
        .expect("加载应用图标失败")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // 启动时给一个合理默认值；首帧会按屏幕尺寸再自适应
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([720.0, 480.0])
            .with_maximized(false)
            .with_title("Askway")
            .with_icon(Arc::new(load_app_icon())),
        ..Default::default()
    };

    eframe::run_native(
        "Askway",
        options,
        Box::new(|cc| Ok(Box::new(AskwayApp::new(cc)))),
    )
}
