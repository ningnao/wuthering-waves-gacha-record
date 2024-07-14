use std::cmp::min;
use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use eframe::epaint::Stroke;
use eframe::glow::Context;
use crate::gacha_statistics;
use egui::{CentralPanel, Color32, ComboBox, FontData, FontId, TextStyle, Vec2, Vec2b, Visuals};
use egui::FontFamily::{Monospace, Proportional};
use egui_plot::{Bar, BarChart, Corner, Legend, Plot};
use tracing::{error, info, warn};
use crate::core::message::MessageType;
use crate::core::message::MessageType::{Gacha, Normal, Player, Update, Warning};
use crate::core::statistics::{gacha_statistics_from_cache, GachaStatistics, GachaStatisticsDataItem};
use crate::core::util::get_player_id_vec;

fn setup_custom_fonts(ctx: &egui::Context) {
    // Start with the default fonts (we will be adding to them rather than replacing them).
    let mut fonts = egui::FontDefinitions::default();

    // 使用 得意黑 作为 UI 字体
    fonts.font_data.insert("SmileySans".to_owned(), FontData::from_static(include_bytes!("../resource/fonts/SmileySans-Oblique.otf")));
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
    view_tx: Sender<MessageType>,
    view_rx: Receiver<MessageType>,
    on_exit: Arc<AtomicBool>,
    dark_mode: bool,

    gacha_statistics: GachaStatistics,
    gacha_statistic_view_vec: Vec<GachaStatisticsView>,
    player_id_vec: Vec<String>,
    player_id_selected: String,
    player_id_last_selected: String,
    message: Message,
}

#[derive(Default)]
struct Message {
    success: bool,
    message: String,
}

impl MainView {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);

        // 服务发送 / 视图接收 通道
        let (service_tx, view_rx) = mpsc::channel();
        // 视图发送 / 服务接收 通道
        let (view_tx, service_rx) = mpsc::channel();

        let on_exit_flag = Arc::new(AtomicBool::new(false));

        start_data_flush_thread(Arc::clone(&on_exit_flag), service_tx, service_rx);

        let _ = view_tx.send(Update(true, "".to_string()));

        Self {
            view_tx,
            view_rx,
            on_exit: on_exit_flag,
            dark_mode: false,
            gacha_statistics: GachaStatistics::new(),
            gacha_statistic_view_vec: vec![],
            player_id_vec: vec![],
            player_id_last_selected: String::default(),
            player_id_selected: String::default(),
            message: Message::default(),
        }
    }
}

fn start_data_flush_thread(on_exit_flag_clone: Arc<AtomicBool>,
                           service_tx: Sender<MessageType>,
                           service_rx: Receiver<MessageType>) {
    tokio::spawn(async move {
        loop {
            if on_exit_flag_clone.load(Ordering::Relaxed) {
                info!("应用退出，停止后台线程");
                break;
            }

            if let Ok(message) = service_rx.recv_timeout(Duration::from_secs(1)) {
                match message {
                    Update(cache, mut player_id) => {
                        let _ = service_tx.send(Normal("加载中...".to_string()));
                        if cache {
                            // 从缓存中获取数据
                            if let Ok(user_vec) = get_player_id_vec() {
                                if !user_vec.is_empty() {
                                    if player_id.is_empty() {
                                        player_id = user_vec[0].clone();
                                        let _ = service_tx.send(Player(user_vec));
                                    }

                                    // 第一次加载时尝试读缓存文件中的统计内容，加快首屏加载速度
                                    match gacha_statistics_from_cache(player_id.clone()) {
                                        Ok(gacha_statistics_data) => {
                                            let _ = service_tx.send(Gacha((player_id, gacha_statistics_data)));
                                            let _ = service_tx.send(Normal("当前展示的是该用户最后一次获取的数据".to_string()));
                                            info!("刷新统计图");
                                            continue;
                                        }
                                        Err(err) => {
                                            let _ = service_tx.send(Warning("无缓存，正在尝试从服务器获取".to_string()));
                                            info!("无缓存：{}", err);
                                        }
                                    }
                                }
                            }

                            let _ = service_tx.send(Warning("首次使用，正在尝试从服务器获取".to_string()));
                        }

                        // 从服务器获取抽卡数据
                        match gacha_statistics(player_id, &service_tx).await {
                            Ok(gacha_statistics_data) => {
                                let _ = service_tx.send(Gacha(gacha_statistics_data));
                                let _ = service_tx.send(Normal("获取完毕".to_string()));
                                info!("刷新统计图");

                                // 刷新当前保存数据的用户列表
                                if let Ok(user_vec) = get_player_id_vec() {
                                    let _ = service_tx.send(Player(user_vec));
                                }
                            }
                            Err(err) => {
                                let _ = service_tx.send(Warning(format!("抽卡数据统计失败，失败原因：{}", err)));
                                error!("抽卡数据统计失败：{}", err);
                            }
                        }
                    }
                    _ => {
                        warn!("接收到了错误的消息");
                    }
                }
            }
        }
    });
}

impl eframe::App for MainView {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(message) = self.view_rx.try_recv() {
            match message {
                Normal(message) => {
                    self.message = Message {
                        success: true,
                        message,
                    }
                }
                Warning(message) => {
                    self.message = Message {
                        success: false,
                        message,
                    }
                }
                Gacha((player_id, gacha_statistic)) => {
                    self.player_id_selected = player_id.clone();
                    self.player_id_last_selected = player_id;
                    self.gacha_statistics = gacha_statistic;
                }
                Player(player_id_vec) => {
                    if !player_id_vec.is_empty() {
                        for player_id in player_id_vec.clone() {
                            if !self.player_id_vec.contains(&player_id) {
                                self.player_id_selected = player_id.clone();
                                self.player_id_last_selected = player_id;
                            }
                        }
                    }

                    self.player_id_vec = player_id_vec;
                }
                _ => {
                    warn!("接收到了错误的消息");
                }
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            // 定时刷新内容
            ctx.request_repaint_after(Duration::from_millis(100));

            ui.horizontal(|ui| {
                // 切换显示模式
                let switch_style_button_text = if self.dark_mode { "🌙" } else { "☀" };
                let switch_style_button = ui.button(switch_style_button_text);
                if switch_style_button.clicked() {
                    let mut style = (*ctx.style()).clone();
                    if self.dark_mode {
                        self.dark_mode = false;
                        style.visuals = Visuals::light();
                    } else {
                        self.dark_mode = true;
                        style.visuals = Visuals::dark();
                    }
                    ctx.set_style(style);
                }

                let update_button = ui.button("获取数据更新");
                if update_button.clicked() {
                    info!("开始刷新数据...");
                    let _ = &self.view_tx.send(Update(false, self.player_id_selected.clone()));
                    let _ = &self.gacha_statistic_view_vec.clear();
                }

                // 监听选项变化
                if self.player_id_last_selected.ne(&self.player_id_selected) {
                    // 刷新数据
                    self.player_id_last_selected = self.player_id_selected.clone();
                    let _ = &self.view_tx.send(Update(true, self.player_id_selected.clone()));
                    let _ = self.gacha_statistic_view_vec.clear();
                }

                ui.label("选择用户:");
                ComboBox::from_id_source("player_id")
                    .selected_text(&self.player_id_selected)
                    .show_ui(ui, |ui| {
                        for player_id in self.player_id_vec.clone() {
                            ui.selectable_value(&mut self.player_id_selected, player_id.clone(), player_id);
                        }
                    },
                    );

                let add_user_button = ui.button("获取新用户");
                if add_user_button.clicked() {
                    info!("开始获取新用户...");
                    let _ = &self.view_tx.send(Update(false, "".to_string()));
                    let _ = &self.gacha_statistic_view_vec.clear();
                }

                if self.message.success {
                    ui.label(&self.message.message);
                } else {
                    ui.colored_label(Color32::from_rgb(232, 176, 4), &self.message.message);
                }
            });

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

    fn on_exit(&mut self, _gl: Option<&Context>) {
        info!("应用退出...");
        self.on_exit.swap(true, Ordering::Relaxed);
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
                    .width(0.95)
                    .fill(Color32::from_rgb(129, 206, 255))
                    .stroke(Stroke::new(1.5, Color32::from_rgb(99, 176, 225)));
                let bar_chart = BarChart::new(vec![bar])
                    .name("3星")
                    .color(Color32::from_rgb(129, 206, 255));
                bar_chart_vec.push(bar_chart);

                let bar = Bar::new(2f64, gacha_statistics_data.four_count as f64)
                    .width(0.95)
                    .fill(Color32::from_rgb(201, 131, 237))
                    .stroke(Stroke::new(1.5, Color32::from_rgb(171, 101, 207)));
                let bar_chart = BarChart::new(vec![bar])
                    .name("4星")
                    .color(Color32::from_rgb(201, 131, 237));
                bar_chart_vec.push(bar_chart);

                let bar = Bar::new(3f64, gacha_statistics_data.five_count as f64)
                    .width(0.95)
                    .fill(Color32::from_rgb(255, 246, 145))
                    .stroke(Stroke::new(1.5, Color32::from_rgb(225, 216, 115)));
                let bar_chart = BarChart::new(vec![bar])
                    .name("5星")
                    .color(Color32::from_rgb(255, 246, 145));
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