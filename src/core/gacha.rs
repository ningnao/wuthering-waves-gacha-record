use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::sync::mpsc::Sender;
use anyhow::Error;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::core::message::MessageType;
use crate::core::message::MessageType::Normal;
use crate::core::util;

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
}
 */
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GachaData {
    card_pool_type: String,
    pub(crate) resource_id: i32,
    pub(crate) quality_level: i32,
    pub(crate) resource_type: String,
    pub(crate) name: String,
    count: i32,
    time: String,
}

pub(crate) type SavedGachaData = BTreeMap<i32, Vec<GachaData>>;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestParam {
    // 同 resources_id
    card_pool_id: String,
    // 卡池类型（写代码时范围为 1-7）
    card_pool_type: i32,
    // 固定为 zh-Hans
    language_code: String,
    // 用户 ID
    pub(crate) player_id: String,
    record_id: String,
    // 同 svr_id
    server_id: String,
}

impl RequestParam {
    pub(crate) fn init(card_pool_id: String, language_code: String, player_id: String, record_id: String, server_id: String) -> Self {
        Self {
            card_pool_id,
            card_pool_type: 0,
            language_code,
            player_id,
            record_id,
            server_id,
        }
    }
}

pub(crate) async fn get_gacha_data(player_id: String, server_sender: &Sender<MessageType>) -> Result<(String, SavedGachaData), Error> {
    // 从日志文件中获取抽卡记录 API 所需要的请求参数
    let (oversea, mut param) = util::get_param_from_logfile(player_id, server_sender)?;

    let _ = fs::create_dir_all(format!("./data/{}", param.player_id));
    let file_path = format!("./data/{}/gacha_data.json", param.player_id);

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

    // 适配国际服
    // 国服：gmserver-api.aki-game2.com
    // 国际服：gmserver-api.aki-game2.net
    let url = if oversea {
        "https://gmserver-api.aki-game2.net/gacha/record/query"
    } else {
        "https://gmserver-api.aki-game2.com/gacha/record/query"
    };

    for card_pool_type in 1..=7 {
        let _ = server_sender.send(Normal(format!("正在获取卡池 {} 的数据", card_pool_type)));
        param.card_pool_type = card_pool_type;

        let result = reqwest::Client::new()
            .post(url)
            .json(&param)
            .send()
            .await;

        match result {
            Ok(res) => {
                let body = res.json::<CommonResult>().await?;
                if body.code != 0 {
                    // 接口请求失败，可能是请求参数变化，删除 url 缓存，下次重新获取
                    let _ = fs::remove_file(format!("./data/{}/url_cache.txt", param.player_id));
                    return Err(Error::msg("抽卡链接可能已经失效，请打开抽卡页面后重新获取。"));
                }

                let mut default = vec![];
                let saved_gacha_data_by_type = saved_gacha_data.get_mut(&card_pool_type)
                    .unwrap_or(&mut default);

                let mut gacha_data_by_type = vec![];
                for gacha_data in body.data {
                    if saved_gacha_data_by_type.contains(&gacha_data) {
                        info!("卡池 {} 已经到达上次记录位置，停止记录", card_pool_type);
                        break;
                    }

                    // 卡池顺序为时间倒序，此处调整为顺序
                    gacha_data_by_type.insert(0, gacha_data.clone());
                }

                // 在旧数据后追加新数据
                let mut saved_gacha_data_by_type = saved_gacha_data_by_type.clone();
                saved_gacha_data_by_type.append(&mut gacha_data_by_type);
                // 保存
                saved_gacha_data.insert(card_pool_type, saved_gacha_data_by_type);
            }
            Err(err) => {
                return Err(Error::msg(format!("网络连接异常：{}", err)));
            }
        }
    }

    let _ = fs::create_dir_all(format!("./data/{}/backup", param.player_id));
    // 刷新数据前备份数据
    fs::copy(&file_path, format!("./data/{}/backup/gacha_data.json.{}.backup",
                                 param.player_id, Local::now().format("%Y-%m-%d-%H-%M-%S-%6f")))?;

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(file_path)?;

    let _ = &file.write_all(&*serde_json::to_vec(&saved_gacha_data)?)?;

    Ok((param.player_id, saved_gacha_data))
}