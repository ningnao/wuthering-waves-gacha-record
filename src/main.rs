use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};

use anyhow::{Error, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sysinfo::System;
use url::Url;

/* TODO LIST
    - 增加图形界面
    - 保存用户信息
    - 使用日志库
    - 优化代码
 */

// 接口统一返回值
#[derive(Serialize, Deserialize, Debug)]
struct CommonResult {
    code: i32,
    message: String,
    data: Vec<GachaData>,
}

/* 具体的抽卡数据
{
    "card_pool_type":"角色精准调谐",
    "resourceId":21010043,
    "qualityLevel":3,
    "resourceType":"武器",
    "name":"远行者长刃·辟路",
    "count":1,
    "time":"2024-07-05 07:40:58"
} */
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GachaData {
    card_pool_type: String,
    resource_id: i32,
    quality_level: i32,
    resource_type: String,
    name: String,
    count: i32,
    time: String,
}

type SavedGachaData = BTreeMap<i32, Vec<GachaData>>;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RequestParam {
    // 同 resources_id
    card_pool_id: String,
    // 卡池类型（写代码时范围为 1-7）
    card_pool_type: i32,
    // 固定为 zh-Hans
    language_code: String,
    // 用户 ID
    player_id: String,
    record_id: String,
    // 同 svr_id
    server_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // 动态获取游戏目录及日志文件
    let logfile_path = get_wuthering_waves_progress_path()?;
    // 从日志文件中获取抽卡历史记录 url
    let url = get_url_from_logfile(logfile_path)?;
    // 从抽卡历史记录 url 中获取抽卡记录 API 所需要的请求参数
    let param = get_request_param(url.to_string())?;

    let mut param = RequestParam {
        card_pool_id: param.get("resources_id").unwrap_or(&"".to_string()).to_string(),
        card_pool_type: 0,
        language_code: "zh-Hans".to_string(),
        player_id: param.get("player_id").unwrap_or(&"".to_string()).to_string(),
        record_id: param.get("record_id").unwrap_or(&"".to_string()).to_string(),
        server_id: param.get("svr_id").unwrap_or(&"".to_string()).to_string(),
    };

    let file_path = String::from(format!("/data/gacha_data_{}.json", param.player_id));

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&file_path)?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    let mut saved_gacha_data;
    if !buffer.is_empty() {
        saved_gacha_data = serde_json::from_str::<SavedGachaData>(&*buffer)?;
    } else {
        saved_gacha_data = SavedGachaData::default();
    }

    for card_pool_type in 1..=7 {
        param.card_pool_type = card_pool_type;

        let result = reqwest::Client::new()
            .post("https://gmserver-api.aki-game2.com/gacha/record/query")
            .json(&param)
            .send()
            .await;

        match result {
            Ok(res) => {
                let body = res.json::<CommonResult>().await?;
                if body.code != 0 {
                    // 接口请求失败直接返回
                    return Err(Error::msg("获取抽卡信息失败！"));
                }

                let default = Vec::new();
                let last_data = saved_gacha_data.get(&card_pool_type)
                    .unwrap_or(&default);

                let mut need_save = Vec::new();
                for item in body.data {
                    if last_data.contains(&item) {
                        println!("已经到达上次记录位置，停止记录");
                        break;
                    }

                    need_save.insert(0, item.clone());
                }

                // TODO 优化
                let mut data = last_data.clone();
                let mut data = data.as_mut();
                need_save.append(&mut data);

                saved_gacha_data.insert(card_pool_type, need_save);
            }
            Err(err) => {
                return Err(Error::from(err));
            }
        }
    }

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(file_path)?;

    let _ = &file.write_all(&*serde_json::to_vec(&saved_gacha_data)?);

    gacha_statistics(saved_gacha_data);
    Ok(())
}

fn get_wuthering_waves_progress_path() -> Result<String, Error> {
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
    println!("{}", path);
}

fn get_url_from_logfile(logfile_path: String) -> Result<String, Error> {
    println!("解析到的日志路径：{}", logfile_path);
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
fn get_request_param(url: String) -> Result<HashMap<String, String>, Error> {
    // 清除 url 中的 #
    let url = url.replace("#", "");
    let url = Url::parse(&*url)?;
    let param = url.query_pairs();
    Ok(param.into_owned().collect::<HashMap<_, _>>())
}

#[test]
fn get_request_param_test() {
    let url = "https://aki-gm-resources.aki-game.com/aki/gacha/index.html#/record?svr_id=***&player_id=***&lang=zh-Hans&gacha_id=100003&gacha_type=1&svr_area=cn&record_id=***&resources_id=***";
    let param = get_request_param(url.to_string());

    println!("{:?}", param);
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GachaStatisticsData {
    card_pool_type: i32,
    total: i32,
    five_count: i32,
    four_count: i32,
    three_count: i32,
    pull_count: i32,
    detail: Vec<GachaStatisticsDataItem>,
}

// 抽卡统计详情
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GachaStatisticsDataItem {
    name: String,
    count: i32,
    resource_type: String,
}

type GachaStatistics = BTreeMap<i32, GachaStatisticsData>;

fn gacha_statistics(gacha_data: SavedGachaData) {
    let mut statistics = GachaStatistics::new();

    for (card_pool_type, data) in gacha_data {
        let mut statistics_data = GachaStatisticsData {
            card_pool_type,
            total: 0,
            five_count: 0,
            four_count: 0,
            three_count: 0,
            pull_count: 0,
            detail: vec![],
        };

        // 累计抽数（出金清零）
        let mut inner_count = 0;
        // 出金抽数（用于统计未出金抽数）
        let mut get_five_pull_count = 0;
        for item in data {
            inner_count += 1;
            statistics_data.total += 1;

            match item.quality_level {
                5 => {
                    statistics_data.five_count += 1;

                    statistics_data.detail.push(GachaStatisticsDataItem {
                        name: item.name,
                        count: inner_count,
                        resource_type: item.resource_type,
                    });

                    get_five_pull_count += inner_count;
                    inner_count = 0;
                }
                4 => { statistics_data.four_count += 1; }
                3 => { statistics_data.three_count += 1; }
                _ => {}
            }
            statistics_data.pull_count = statistics_data.total - get_five_pull_count;

            statistics.insert(card_pool_type, statistics_data.clone());
        }
    }

    println!("{:?}", statistics);
}