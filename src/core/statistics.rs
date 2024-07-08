use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::core::gacha::SavedGachaData;

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

pub(crate) fn gacha_statistics(gacha_data: SavedGachaData) {
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

    info!("{:?}", statistics);
}