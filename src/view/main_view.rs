use crate::core::message::MessageType;
use crate::core::message::MessageType::{
    CheckUpdate, DownloadFile, Gacha, NeedUpdate, Normal, Player, UpdateData, Warning,
};
use crate::core::statistics::{
    gacha_statistics_from_cache, GachaStatistics, GachaStatisticsDataItem,
};
use crate::core::update::{check_update, download_file, Release};
use crate::core::util::get_player_id_vec;
use crate::gacha_statistics;
use crate::widgets::pie_chart::PieChart;
use egui::FontFamily::Proportional;
use egui::{CentralPanel, Color32, ComboBox, FontData, FontId, TextStyle, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use eframe::Frame;
use egui::scroll_area::ScrollSource;
use egui_theme_switch::global_theme_switch;
use tracing::{error, info, warn};

#[derive(Serialize, Deserialize)]
struct Config {
    dark_mode: bool,
    player_id_selected: String,
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
    update_info: Option<Release>,
    view: View,

    markdown_cache: CommonMarkCache,
}

#[derive(Default)]
struct Message {
    success: bool,
    message: String,
}

enum View {
    Home,
    Update,
}

impl MainView {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 初始化数据处理线程
        // 服务发送 / 视图接收 通道
        let (service_tx, view_rx) = mpsc::channel();
        // 视图发送 / 服务接收 通道
        let (view_tx, service_rx) = mpsc::channel();

        let on_exit_flag = Arc::new(AtomicBool::new(false));

        start_data_flush_thread(Arc::clone(&on_exit_flag), service_tx, service_rx);

        let _ = view_tx.send(UpdateData(true, "".to_string()));
        let _ = view_tx.send(CheckUpdate);

        let mut dark_mode = false;
        let mut player_id_selected = String::default();

        // 读取用户配置
        if let Ok(config) = fs::read_to_string("./data/config.toml") {
            if let Ok(config) = toml::from_str::<Config>(config.as_str()) {
                dark_mode = config.dark_mode;
                player_id_selected = config.player_id_selected;
            }
        }

        // 样式配置
        setup_custom_style(&cc.egui_ctx, dark_mode);

        Self {
            view_tx,
            view_rx,
            on_exit: on_exit_flag,
            dark_mode,
            gacha_statistics: GachaStatistics::new(),
            gacha_statistic_view_vec: vec![],
            player_id_vec: vec![],
            player_id_last_selected: String::default(),
            player_id_selected,
            message: Message::default(),
            update_info: None,
            view: View::Home,
            markdown_cache: CommonMarkCache::default(),
        }
    }
}

fn setup_custom_style(ctx: &egui::Context, _dark_mode: bool) {
    let mut fonts = egui::FontDefinitions::default();

    // 使用 得意黑 作为 UI 字体
    fonts.font_data.insert(
        "SmileySans".to_owned(),
        Arc::from(FontData::from_static(include_bytes!("../resource/fonts/SmileySans-Oblique.otf"))),
    );

    fonts
        .families
        .get_mut(&Proportional)
        .unwrap()
        .insert(0, "SmileySans".to_owned());

    ctx.set_fonts(fonts);

    // 设置字体默认样式
    let text_styles: BTreeMap<TextStyle, FontId> = [
        (TextStyle::Heading, FontId::new(25.0, Proportional)),
        (TextStyle::Body, FontId::new(16.0, Proportional)),
        (TextStyle::Monospace, FontId::new(16.0, Proportional)),
        (TextStyle::Button, FontId::new(16.0, Proportional)),
        (TextStyle::Small, FontId::new(8.0, Proportional)),
    ]
    .into();

    ctx.all_styles_mut(move |style| {
        style.text_styles = text_styles.clone()
    });
}

fn start_data_flush_thread(
    on_exit_flag_clone: Arc<AtomicBool>,
    service_tx: Sender<MessageType>,
    service_rx: Receiver<MessageType>,
) {
    tokio::spawn(async move {
        loop {
            if on_exit_flag_clone.load(Ordering::Relaxed) {
                info!("应用退出，停止后台线程");
                break;
            }

            if let Ok(message) = service_rx.recv_timeout(Duration::from_secs(1)) {
                match message {
                    CheckUpdate => {
                        info!("检查应用更新");
                        if let Ok(update_info) = check_update().await {
                            info!("程序有更新");
                            let _ = service_tx.send(NeedUpdate(update_info));
                        } else {
                            info!("当前已是最新版本");
                        }
                    }
                    DownloadFile(release, filepath) => {
                        let _ = service_tx.send(Normal("正在连接服务器...".to_string()));
                        match download_file(release, filepath, service_tx.clone()).await {
                            Ok(_) => {
                                info!("更新包下载完毕");
                                let _ = service_tx.send(Normal("下载完毕 100%".to_string()));
                            }
                            Err(err) => {
                                let _ = service_tx.send(Warning(format!("下载失败：{}", err)));
                            }
                        }
                    }
                    UpdateData(cache, mut player_id) => {
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
                                            let _ = service_tx
                                                .send(Gacha((player_id, gacha_statistics_data)));
                                            let _ = service_tx.send(Normal(
                                                "当前展示的是该用户最后一次获取的数据".to_string(),
                                            ));
                                            info!("刷新统计图");
                                            continue;
                                        }
                                        Err(err) => {
                                            let _ = service_tx.send(Warning(
                                                "无缓存，正在尝试从服务器获取".to_string(),
                                            ));
                                            info!("无缓存：{}", err);
                                        }
                                    }
                                }
                            }

                            let _ = service_tx
                                .send(Warning("首次使用，正在尝试从服务器获取".to_string()));
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
                                let _ = service_tx
                                    .send(Warning(format!("抽卡数据统计失败，失败原因：{}", err)));
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
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        // 定时刷新内容
        ui.request_repaint_after(Duration::from_millis(100));

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
                NeedUpdate(update_info) => {
                    self.update_info = Some(update_info);
                }
                _ => {
                    warn!("接收到了错误的消息");
                }
            }
        }

        CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                // 切换显示模式
                global_theme_switch(ui);

                let update_button = ui.button("获取数据更新");
                if update_button.clicked() {
                    info!("开始刷新数据...");
                    let _ = &self
                        .view_tx
                        .send(UpdateData(false, self.player_id_selected.clone()));
                    let _ = &self.gacha_statistic_view_vec.clear();
                }

                // 监听选项变化
                if self.player_id_last_selected.ne(&self.player_id_selected) {
                    // 刷新数据
                    self.player_id_last_selected = self.player_id_selected.clone();
                    let _ = &self
                        .view_tx
                        .send(UpdateData(true, self.player_id_selected.clone()));
                    let _ = self.gacha_statistic_view_vec.clear();
                }

                ui.label("选择用户:");
                ComboBox::from_id_salt("player_id")
                    .selected_text(&self.player_id_selected)
                    .show_ui(ui, |ui| {
                        for player_id in self.player_id_vec.clone() {
                            ui.selectable_value(
                                &mut self.player_id_selected,
                                player_id.clone(),
                                player_id,
                            );
                        }
                    });

                let add_user_button = ui.button("获取新用户");
                if add_user_button.clicked() {
                    info!("开始获取新用户...");
                    let _ = &self.view_tx.send(UpdateData(false, "".to_string()));
                    let _ = &self.gacha_statistic_view_vec.clear();
                }

                if let Some(update_info) = &self.update_info {
                    let new_version = ui.button(format!("新版本 {}", update_info.tag_name));
                    if new_version.clicked() {
                        self.view = View::Update;
                    }
                }

                if self.message.success {
                    ui.label(&self.message.message);
                } else {
                    ui.colored_label(Color32::from_rgb(232, 176, 4), &self.message.message);
                }
            });

            if let View::Home = self.view {
                // 刷新统计图内容
                let _ = &self.create_bar_chart(&self.gacha_statistics.clone());
                let gacha_statistic_view_vec = &mut self.gacha_statistic_view_vec;

                egui::ScrollArea::vertical()
                    .scroll_source(ScrollSource::MOUSE_WHEEL)
                    .show(ui, |ui| {
                        for _ in 0..(gacha_statistic_view_vec.len() as f32 / 3.0).ceil() as i32 {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.group(|ui| {
                                        for _ in 0..min(3, gacha_statistic_view_vec.len() as i32) {
                                            let mut item = gacha_statistic_view_vec.remove(0);
                                            ui.vertical(|ui| {
                                                match item.card_pool_type {
                                                    1 => {
                                                        ui.label("角色活动唤取");
                                                    }
                                                    2 => {
                                                        ui.label("武器活动唤取");
                                                    }
                                                    3 => {
                                                        ui.label("角色常驻唤取");
                                                    }
                                                    4 => {
                                                        ui.label("武器常驻唤取");
                                                    }
                                                    5 => {
                                                        ui.label("新手唤取");
                                                    }
                                                    6 => {
                                                        ui.label("新手自选唤取");
                                                    }
                                                    7 => {
                                                        ui.label("新手自选唤取（感恩定向唤取）");
                                                    }
                                                    8 => {
                                                        ui.label("角色新旅唤取");
                                                    }
                                                    9 => {
                                                        ui.label("武器新旅唤取");
                                                    }
                                                    10 => {
                                                        ui.label("角色联动唤取");
                                                    }
                                                    11 => {
                                                        ui.label("武器联动唤取");
                                                    }
                                                    _ => {
                                                        ui.label("新卡池");
                                                    }
                                                }
                                                item.pie_chart.show(ui);

                                                ui.label(format!(
                                                    "当前累计[{}]抽，已垫[{}]抽，5星[{}]个",
                                                    item.total,
                                                    item.pull_count,
                                                    item.detail.len()
                                                ));
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.set_max_width(285.0);
                                                    for item in item.detail {
                                                        ui.label(format!(
                                                            "{}[{}]",
                                                            item.name, item.count
                                                        ));
                                                    }
                                                });
                                            });
                                        }
                                    });
                                });
                            });
                        }
                    });
            }

            if let View::Update = self.view {
                if let Some(update_info) = &self.update_info {
                    ui.vertical_centered_justified(|ui| {
                        ui.group(|ui| {
                            ui.label(format!("发现新版本：{}", update_info.tag_name));
                            ui.label("");
                            ui.label("更新日志：");
                            CommonMarkViewer::new().show(
                                ui,
                                &mut self.markdown_cache,
                                update_info.body.as_str(),
                            );
                            ui.label("");

                            let download_button = ui.button("下载更新");
                            if download_button.clicked() {
                                self.view = View::Update;
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_title("请选择更新包存放位置")
                                    .pick_folder()
                                {
                                    let picked_path = path.display().to_string();
                                    info!("选择的文件 {:?}", picked_path);
                                    let _ = &self
                                        .view_tx
                                        .send(DownloadFile(update_info.clone(), picked_path));
                                } else {
                                    self.message = Message {
                                        success: true,
                                        message: "用户取消升级...".to_string(),
                                    }
                                }
                            }
                            let cancel_button = ui.button("取消更新");
                            if cancel_button.clicked() {
                                self.view = View::Home;
                            }
                        });
                    });
                } else {
                    self.view = View::Home;
                }
            }
        });
    }

    fn on_exit(&mut self) {
        info!("应用退出...");
        self.on_exit.swap(true, Ordering::Relaxed);
        info!("储存用户配置...");
        let _ = fs::create_dir_all("./data");
        if let Ok(config_str) = toml::to_string(&Config {
            dark_mode: self.dark_mode,
            player_id_selected: self.player_id_selected.clone(),
        }) {
            if let Ok(mut file) = OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open("./data/config.toml")
            {
                let _ = file.write_all(config_str.as_bytes());
            }
        }
    }
}

struct GachaStatisticsView {
    card_pool_type: i32,
    total: i32,
    pull_count: i32,
    pie_chart: PieChart,
    detail: Vec<GachaStatisticsDataItem>,
}

impl MainView {
    fn create_bar_chart(&mut self, gacha_statistic: &GachaStatistics) {
        if self.gacha_statistic_view_vec.is_empty() {
            let mut gacha_statistic_view_vec = vec![];
            for (card_pool_type, gacha_statistics_data) in gacha_statistic.iter() {
                let pie_chart = PieChart::new(
                    gacha_statistics_data.card_pool_type.to_string(),
                    vec![
                        (
                            gacha_statistics_data.three_count as f64,
                            "3星".to_string(),
                            Color32::from_rgb(99, 176, 225),
                        ),
                        (
                            gacha_statistics_data.four_count as f64,
                            "4星".to_string(),
                            Color32::from_rgb(171, 101, 207),
                        ),
                        (
                            gacha_statistics_data.five_count as f64,
                            "5星".to_string(),
                            Color32::from_rgb(225, 216, 115),
                        ),
                    ],
                );

                let gacha_statistic_view = GachaStatisticsView {
                    card_pool_type: *card_pool_type,
                    total: gacha_statistics_data.total,
                    pull_count: gacha_statistics_data.pull_count,
                    pie_chart,
                    detail: gacha_statistics_data.detail.clone(),
                };

                gacha_statistic_view_vec.push(gacha_statistic_view);
            }

            self.gacha_statistic_view_vec = gacha_statistic_view_vec;
        }
    }
}
