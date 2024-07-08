use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use anyhow::Error;
use regex::Regex;
use sysinfo::System;
use tracing::info;
use url::Url;
use crate::core::gacha::RequestParam;

pub(crate) fn get_wuthering_waves_progress_path() -> anyhow::Result<String, Error> {
    let mut system = System::new();
    system.refresh_all();

    // TODO 优化
    for process in system.processes_by_name("launcher.exe") {
        if let Some(path) = process.exe() {
            if let Some(path) = path.parent() {
                if let Some(path) = path.to_str() {
                    let path = path.to_owned() + r#"\Wuthering Waves Game\Client\Saved\Logs\Client.log"#;
                    if fs::metadata(&path).is_ok() {
                        return Ok(path.clone());
                    }
                }
            }
        }
    }

    for process in system.processes_by_name("Wuthering Waves.exe") {
        if let Some(path) = process.exe() {
            if let Some(path) = path.parent() {
                if let Some(path) = path.to_str() {
                    let path = path.to_owned() + r#"\Client\Saved\Logs\Client.log"#;
                    if fs::metadata(&path).is_ok() {
                        return Ok(path.clone());
                    }
                }
            }
        }
    }

    Err(Error::msg("未找到游戏进程！"))
}

#[test]
fn get_wuthering_waves_progress_path_test() {
    let path = get_wuthering_waves_progress_path().unwrap();
    info!("{}", path);
}

pub(crate) fn get_url_from_logfile(logfile_path: String) -> anyhow::Result<String, Error> {
    info!("解析到的日志路径：{}", logfile_path);
    let mut file = OpenOptions::new()
        .read(true)
        .open(logfile_path)?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    let regex = Regex::new("https.*/aki/gacha/index.html#/record[?=&\\w\\-]+")?;
    let url = regex.find_iter(&*buffer).last();
    let url = match url {
        Some(url) => {
            url.as_str().to_string()
        }
        None => {
            return Err(Error::msg("未打开过抽卡页面！"));
        }
    };

    Ok(url)
}

#[test]
fn get_url_from_logfile_test() {
    let logfile_path = get_wuthering_waves_progress_path().unwrap();
    get_url_from_logfile(logfile_path).unwrap();
}

// https://aki-gm-resources.aki-game.com/aki/gacha/index.html#/record?svr_id=***&player_id=***&lang=zh-Hans&gacha_id=100003&gacha_type=1&svr_area=cn&record_id=***&resources_id=***
pub(crate) fn get_request_param(url: String) -> Result<RequestParam, Error> {
    // 清除 url 中的 #
    let url = url.replace("#", "");
    let url = Url::parse(&*url)?;
    let param = url.query_pairs();

    let mut resources_id = String::new();
    let mut player_id = String::new();
    let mut record_id = String::new();
    let mut svr_id = String::new();
    for (key, value) in param.into_iter() {
        match key.to_string().as_str() {
            "resources_id" => { resources_id = value.to_string(); }
            "player_id" => { player_id = value.to_string(); }
            "record_id" => { record_id = value.to_string(); }
            "svr_id" => { svr_id = value.to_string(); }
            _ => {}
        }
    }
    Ok(RequestParam::init(resources_id, player_id, record_id, svr_id))
}

#[test]
fn get_request_param_test() {
    let url = "https://aki-gm-resources.aki-game.com/aki/gacha/index.html#/record?svr_id=***&player_id=***&lang=zh-Hans&gacha_id=100003&gacha_type=1&svr_area=cn&record_id=***&resources_id=***";
    let param = get_request_param(url.to_string());

    info!("{:?}", param);
}