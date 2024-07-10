// hide console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::Renderer::Wgpu;
use egui::{Style, Visuals};
use crate::core::statistics::gacha_statistics;
use crate::view::main_view::MainView;

mod core;
mod view;
/* TODO LIST
    - 保存用户信息
    - 优化代码
 */

#[tokio::main]
async fn main() -> eframe::Result {
    // 日志初始化
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_resizable(false)
            .with_maximize_button(false)
            .with_inner_size([900.0, 500.0]),
        renderer: Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "鸣潮抽卡记录工具",
        options,
        Box::new(|cc| {
            let style = Style {
                visuals: Visuals::light(),
                ..Style::default()
            };
            cc.egui_ctx.set_style(style);
            Ok(Box::new(MainView::new(cc)))
        }),
    )
}