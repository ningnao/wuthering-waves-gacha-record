use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::core::gacha::get_gacha_data;
use crate::core::message::MessageSender;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GachaStatisticsData {
    pub(crate) card_pool_type: i32,
    pub(crate) total: i32,
    pub(crate) five_count: i32,
    pub(crate) four_count: i32,
    pub(crate) three_count: i32,
    pub(crate) pull_count: i32,
    pub(crate) detail: Vec<GachaStatisticsDataItem>,
}

// 抽卡统计详情
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GachaStatisticsDataItem {
    pub(crate) name: String,
    pub(crate) count: i32,
    pub(crate) resource_id: i32,
    pub(crate) resource_type: String,
}

pub(crate) type GachaStatistics = BTreeMap<i32, GachaStatisticsData>;

pub(crate) async fn gacha_statistics(message_sender: &MessageSender) -> Result<GachaStatistics, Error> {
    // 从服务获取抽卡数据
    let gacha_data = get_gacha_data(message_sender).await?;

    let mut statistics: GachaStatistics = GachaStatistics::new();

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
                        resource_id: item.resource_id,
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

    info!("统计数据完毕");
    // 数据处理完毕后写入缓存文件
    let _ = fs::create_dir_all("./data");
    let file_path = String::from("data/gacha_statistic_cache.json");

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)?;
    file.write_all(&*serde_json::to_vec(&statistics)?)?;

    Ok(statistics)
}

// 从缓存文件中获取统计数据
pub(crate) fn gacha_statistics_from_cache() -> Result<GachaStatistics, Error> {
    let _ = fs::create_dir_all("./data");
    let file_path = String::from("./data/gacha_statistic_cache.json");

    let mut file = OpenOptions::new()
        .read(true)
        .open(&file_path)?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    if buffer.is_empty() {
        return Err(Error::msg("无缓存"));
    }

    let statistics = serde_json::from_str::<GachaStatistics>(&*buffer)?;
    Ok(statistics)
}