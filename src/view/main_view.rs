use std::cmp::min;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::sync::mpsc::Receiver;
use std::time::Duration;
use crate::gacha_statistics;
use egui::{CentralPanel, FontData, FontId, TextStyle, Vec2, Vec2b};
use egui::FontFamily::{Monospace, Proportional};
use egui_plot::{Bar, BarChart, Corner, Legend, Plot};
use tracing::{error, info};
use crate::core::gacha::get_gacha_data;
use crate::core::statistics::{gacha_statistics_from_cache, GachaStatistics, GachaStatisticsDataItem};

fn setup_custom_fonts(ctx: &egui::Context) {
    // Start with the default fonts (we will be adding to them rather than replacing them).
    let mut fonts = egui::FontDefinitions::default();

    // 使用 得意黑 作为 UI 字体
    fonts.font_data.insert("SmileySans".to_owned(), FontData::from_static(include_bytes!("../fonts/SmileySans-Oblique.otf")));
    // Install my own font (maybe supporting non-latin characters).
    // .ttf and .otf files supported.
    fonts.families.get_mut(&Proportional).unwrap().insert(0, "SmileySans".to_owned());

    // Tell egui to use these fonts:
    ctx.set_fonts(fonts);

    // 设置字体默认样式
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(25.0, Proportional)),
        (TextStyle::Body, FontId::new(16.0, Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, Monospace)),
        (TextStyle::Button, FontId::new(16.0, Proportional)),
        (TextStyle::Small, FontId::new(8.0, Proportional)),
    ]
        .into();
    ctx.set_style(style);
}

pub(crate) struct MainView {
    rx: Receiver<GachaStatistics>,
    need_update: Arc<AtomicBool>,
    gacha_statistics: GachaStatistics,
    gacha_statistic_view_vec: Vec<GachaStatisticsView>,
}

impl MainView {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);

        let (tx, rx) = mpsc::channel();

        let need_update = Arc::new(AtomicBool::new(true));
        let need_update_clone = Arc::clone(&need_update);

        tokio::spawn(async move {
            let mut first_flag = true;
            loop {
                if need_update_clone.load(Ordering::Relaxed) {
                    need_update_clone.swap(false, Ordering::Relaxed);
                    if first_flag {
                        first_flag = false;
                        // 第一次加载时尝试读缓存文件中的统计内容，加快首屏加载速度
                        match gacha_statistics_from_cache() {
                            Ok(gacha_statistics_data) => {
                                if let Ok(_) = tx.send(gacha_statistics_data) {
                                    info!("刷新统计图");
                                } else {
                                    error!("数据传输失败！");
                                }
                                continue;
                            }
                            Err(err) => {
                                error!("读取缓存失败：{}", err);
                            }
                        }
                    }

                    match get_gacha_data().await {
                        Ok(gacha_data) => {
                            match gacha_statistics(gacha_data) {
                                Ok(gacha_statistics_data) => {
                                    if let Ok(_) = tx.send(gacha_statistics_data) {
                                        info!("刷新统计图");
                                    } else {
                                        error!("数据传输失败！");
                                    }
                                }
                                Err(err) => {
                                    error!("抽卡数据统计失败：{}", err);
                                }
                            }
                        }
                        Err(err) => {
                            error!("获取抽卡数据失败：{}", err);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        Self {
            rx,
            need_update,
            gacha_statistics: GachaStatistics::new(),
            gacha_statistic_view_vec: vec![],
        }
    }
}

impl eframe::App for MainView {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            // 定时刷新内容
            ctx.request_repaint_after(Duration::from_millis(1000));

            let button = ui.button("获取数据更新");
            if let Ok(data) = self.rx.try_recv() {
                self.gacha_statistics = data;
            }

            if button.clicked() {
                info!("开始刷新数据...");
                let _ = &self.gacha_statistic_view_vec.clear();
                let _ = &self.need_update.swap(true, Ordering::Relaxed);
            }

            // 刷新统计图内容
            let _ = &self.create_bar_chart(&self.gacha_statistics.clone());
            let gacha_statistic_view_vec = &mut self.gacha_statistic_view_vec;

            egui::ScrollArea::vertical().drag_to_scroll(false).show(ui, |ui| {
                for _ in 0..(gacha_statistic_view_vec.len() as f32 / 3.0).ceil() as i32 {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.group(|ui| {
                                for _ in 0..min(3, gacha_statistic_view_vec.len() as i32) {
                                    let item = gacha_statistic_view_vec.remove(0);
                                    ui.vertical(|ui| {
                                        match item.card_pool_type {
                                            1 => { ui.label("角色活动唤取"); }
                                            2 => { ui.label("武器活动唤取"); }
                                            3 => { ui.label("角色常驻唤取"); }
                                            4 => { ui.label("武器常驻唤取"); }
                                            5 => { ui.label("新手唤取"); }
                                            6 => { ui.label("新手自选唤取"); }
                                            7 => { ui.label("新手自选唤取（感恩定向唤取）"); }
                                            _ => { ui.label("新卡池"); }
                                        }
                                        Plot::new(format!("{}", item.card_pool_type))
                                            .legend(Legend::default())
                                            .clamp_grid(true)
                                            .allow_zoom(false)
                                            .allow_drag(false)
                                            .allow_scroll(false)
                                            .allow_boxed_zoom(false)
                                            .show_axes(Vec2b::from([true, false]))
                                            .show_grid(false)
                                            .legend(Legend::default().position(Corner::LeftBottom))
                                            .label_formatter(|_, _| { "".to_owned() })
                                            .width(285.0)
                                            .height(150.0)
                                            .set_margin_fraction(Vec2::from([0.2, 0.2]))
                                            .x_axis_formatter(|mark, _| {
                                                match mark.value as i32 {
                                                    1 => {
                                                        "3星".to_string()
                                                    }
                                                    2 => {
                                                        "4星".to_string()
                                                    }
                                                    3 => {
                                                        "5星".to_string()
                                                    }
                                                    _ => { "".to_owned() }
                                                }
                                            })
                                            .show(ui, |plot_ui| {
                                                for bar_chart in item.bar_chart_vec {
                                                    plot_ui.bar_chart(bar_chart);
                                                }
                                            });
                                        ui.label(format!("当前累计[{}]抽，已垫[{}]抽，5星[{}]个",
                                                         item.total, item.pull_count, item.detail.len()));
                                        ui.horizontal_wrapped(|ui| {
                                            ui.set_max_width(285.0);
                                            for item in item.detail {
                                                ui.label(format!("{}[{}]", item.name, item.count));
                                            }
                                        });
                                    });
                                }
                            });
                        });
                    });
                }
            });
        });
    }
}

struct GachaStatisticsView {
    card_pool_type: i32,
    total: i32,
    pull_count: i32,
    bar_chart_vec: Vec<BarChart>,
    detail: Vec<GachaStatisticsDataItem>,
}

impl MainView {
    fn create_bar_chart(&mut self, gacha_statistic: &GachaStatistics) {
        if self.gacha_statistic_view_vec.is_empty() {
            let mut gacha_statistic_view_vec = vec![];
            for (card_pool_type, gacha_statistics_data) in gacha_statistic.iter() {
                let mut bar_chart_vec = vec![];
                let bar = Bar::new(1f64, gacha_statistics_data.three_count as f64)
                    .width(1.0)
                    .name("3星");
                let bar_chart = BarChart::new(vec![bar])
                    .name("3星");
                // .color(Color32::from_rgb(129,206,255));
                bar_chart_vec.push(bar_chart);

                let bar = Bar::new(2f64, gacha_statistics_data.four_count as f64)
                    .width(1.0)
                    .name("4星");
                let bar_chart = BarChart::new(vec![bar])
                    .name("4星");
                // .color(Color32::from_rgb(201,131,237));
                bar_chart_vec.push(bar_chart);

                let bar = Bar::new(3f64, gacha_statistics_data.five_count as f64)
                    .width(1.0)
                    .name("5星");
                let bar_chart = BarChart::new(vec![bar]).name("5星");
                // .color(Color32::from_rgb(255,246,145));
                bar_chart_vec.push(bar_chart);

                let gacha_statistic_view = GachaStatisticsView {
                    card_pool_type: *card_pool_type,
                    total: gacha_statistics_data.total,
                    pull_count: gacha_statistics_data.pull_count,
                    bar_chart_vec,
                    detail: gacha_statistics_data.detail.clone(),
                };

                gacha_statistic_view_vec.push(gacha_statistic_view);
            }

            self.gacha_statistic_view_vec = gacha_statistic_view_vec;
        }
    }
}