use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::time::SystemTime;
use anyhow::Error;
use regex::Regex;
use sysinfo::System;
use tracing::info;
use url::Url;
use crate::core::gacha::RequestParam;
use crate::core::message::MessageSender;

pub(crate) fn get_wuthering_waves_progress_path() -> anyhow::Result<Vec<String>, Error> {
    let mut system = System::new();
    system.refresh_all();

    let mut log_path = String::new();
    // TODO 优化
    for process in system.processes_by_name("launcher.exe") {
        if let Some(path) = process.exe() {
            if let Some(path) = path.parent() {
                if let Some(path) = path.to_str() {
                    let path = path.to_owned() + r#"\Wuthering Waves Game\Client\Saved\Logs"#;
                    if fs::metadata(&path).is_ok() {
                        log_path = path;
                    }
                }
            }
        }
    }

    if log_path.is_empty() {
        for process in system.processes_by_name("Wuthering Waves.exe") {
            if let Some(path) = process.exe() {
                if let Some(path) = path.parent() {
                    if let Some(path) = path.to_str() {
                        let path = path.to_owned() + r#"\Client\Saved\Logs"#;
                        if fs::metadata(&path).is_ok() {
                            log_path = path;
                        }
                    }
                }
            }
        }
    }

    if log_path.is_empty() {
        return Err(Error::msg("未找到游戏进程"));
    } else {
        let mut path_metadata_vec = vec![];
        let dir = fs::read_dir(&log_path)?;
        for entry in dir.filter_map(Result::ok) {
            if let Ok(metadata) = fs::metadata(entry.path()) {
                if metadata.is_file() {
                    let path = entry.path();
                    let path = path.to_str().unwrap();
                    path_metadata_vec.push((path.to_owned(), metadata.clone()));
                }
            }
        }

        // 按创建时间倒序排序
        path_metadata_vec.sort_by(|(_, metadata_a), (_, metadata_b), | {
            metadata_b
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .cmp(&metadata_a.modified().unwrap_or(SystemTime::UNIX_EPOCH))
        });


        let log_file_vec = path_metadata_vec
            .iter()
            .map(|(path, _)| { path.clone() })
            .collect::<Vec<String>>();

        Ok(log_file_vec)
    }
}

#[test]
fn get_wuthering_waves_progress_path_test() {
    let path = get_wuthering_waves_progress_path().unwrap();
    info!("{:?}", path);
}

pub(crate) fn get_param_from_logfile(player_id: String, message_sender: &MessageSender) -> Result<RequestParam, Error> {
    // 从配置文件中获取历史 url
    let _ = fs::create_dir_all(format!("./data/{}", player_id));
    if let Ok(mut file) = OpenOptions::new().read(true).open(format!("./data/{}/url_cache.txt", player_id)) {
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        if !buffer.is_empty() {
            let param = get_request_param(buffer)?;
            return Ok(param);
        }
    }

    let logfile_path_vec = get_wuthering_waves_progress_path()?;

    for logfile_path in logfile_path_vec {
        // 从路径中截取文件名称用于展示
        let start_index = logfile_path.rfind("\\").unwrap_or_default() + 1;
        let (_, filename) = logfile_path.split_at(start_index);
        message_sender.send(format!("正在从日志文件中获取卡池地址：{}", filename));

        info!("解析到的日志：{}", filename);
        let mut file = OpenOptions::new()
            .read(true)
            .open(logfile_path.clone())?;

        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;

        let regex = Regex::new(r#"https.*/aki/gacha/index.html#/record[?=&\w\-]+"#)?;
        // 匹配最近的那个
        let mut url_vec = vec![];
        for url in regex.find_iter(&*buffer) {
            url_vec.push(url.as_str());
        }
        for url in url_vec.into_iter().rev() {
            // 查找当前选择用户的抽卡 Url
            if !url.contains(player_id.as_str()) {
                info!("Url 与选择用户不匹配");
                continue;
            }

            info!("获取到的卡池 Url：{}", url);

            let param = get_request_param(url.to_string())?;

            // 将获取到的抽卡页面 url 存入文件
            let _ = fs::create_dir_all(format!("./data/{}", param.player_id));
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(format!("./data/{}/url_cache.txt", param.player_id))?;
            file.write_all(url.as_bytes())?;

            return Ok(param);
        }
    }

    Err(Error::msg("未打开过抽卡页面"))
}

#[test]
fn get_url_from_logfile_test() {
    let (tx, _) = std::sync::mpsc::channel();
    let sender = MessageSender::new(tx);

    assert!(get_param_from_logfile("1000000".to_string(), &sender).is_ok());
}

// https://aki-gm-resources.aki-game.com/aki/gacha/index.html#/record?svr_id=***&player_id=***&lang=zh-Hans&gacha_id=100003&gacha_type=1&svr_area=cn&record_id=***&resources_id=***
pub(crate) fn get_request_param(url: String) -> Result<RequestParam, Error> {
    // 清除 url 中的 #
    let url = url.replace("#", "");
    let url = Url::parse(&*url)?;
    let param = url.query_pairs();

    let mut resources_id = String::new();
    let mut lang = String::new();
    let mut player_id = String::new();
    let mut record_id = String::new();
    let mut svr_id = String::new();
    for (key, value) in param.into_iter() {
        match key.to_string().as_str() {
            "resources_id" => { resources_id = value.to_string(); }
            "lang" => { lang = value.to_string(); }
            "player_id" => { player_id = value.to_string(); }
            "record_id" => { record_id = value.to_string(); }
            "svr_id" => { svr_id = value.to_string(); }
            _ => {}
        }
    }
    Ok(RequestParam::init(resources_id, lang, player_id, record_id, svr_id))
}

#[test]
fn get_request_param_test() {
    let url = "https://aki-gm-resources.aki-game.com/aki/gacha/index.html#/record?svr_id=***&player_id=***&lang=zh-Hans&gacha_id=100003&gacha_type=1&svr_area=cn&record_id=***&resources_id=***";
    let param = get_request_param(url.to_string()).unwrap();

    println!("{:?}", param);
}

pub(crate) fn get_player_id_vec() -> Result<Vec<String>, Error> {
    let _ = fs::create_dir_all("./data");
    let data_dir = fs::read_dir("./data")?;
    let mut player_id_vec = vec![];
    for dir in data_dir.filter_map(Result::ok) {
        if dir.metadata()?.is_dir() {
            player_id_vec.push(dir.file_name().into_string().unwrap_or("数据异常".to_string()));
        }
    }

    info!("{:?}", player_id_vec);
    Ok(player_id_vec)
}