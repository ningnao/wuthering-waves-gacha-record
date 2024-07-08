use anyhow::{Error, Result};
use crate::core::gacha::get_gacha_data;
use crate::core::statistics::gacha_statistics;

mod core;

/* TODO LIST
    - 增加图形界面
    - 保存用户信息
    - 优化代码
 */

#[tokio::main]
async fn main() -> Result<(), Error> {
    // 日志初始化
    tracing_subscriber::fmt::init();

    let gacha_data = get_gacha_data().await?;
    gacha_statistics(gacha_data);

    Ok(())
}