use std::sync::mpsc::Sender;
use std::time::Duration;
use anyhow::Error;
use futures_util::stream::StreamExt;
use ratelimit::Ratelimiter;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::core::message::MessageType;
use crate::core::message::MessageType::Normal;
use crate::VERSION;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Release {
    pub(crate) name: String,
    pub(crate) tag_name: String,
    pub(crate) body: String,
    pub(crate) assets: Vec<Assets>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Assets {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

pub(crate) async fn check_update() -> Result<Release, Error> {
    let client = reqwest::Client::new();
    let result = client.get("https://api.github.com/repos/ningnao/wuthering-waves-gacha-record/releases/latest")
        .header("User-Agent", "wuthering-waves-gacha-record")
        .send()
        .await?
        .json::<Release>()
        .await;

    match result {
        Ok(release) => {
            if release.tag_name == format!("v{}", VERSION) {
                return Err(Error::msg("无更新"));
            }
            Ok(release)
        }
        Err(err) => {
            Err(Error::from(err))
        }
    }
}

pub(crate) async fn download_file(release: Release, filepath: String, service_tx: Sender<MessageType>) -> Result<(), Error> {
    let assets = release.assets.get(0).ok_or(Error::msg("获取更新包失败，请重试"))?;

    // 下载升级包
    let client = reqwest::Client::new();
    let response = client.get(&assets.browser_download_url)
        .header("User-Agent", "wuthering-waves-gacha-record")
        .send()
        .await?;

    let content_length = response.content_length().unwrap_or(0);
    let mut downloaded = 0;

    let mut file = tokio::fs::File::create(format!(r#"{}\{}"#, filepath, assets.name)).await?;

    let mut stream = response.bytes_stream();

    let limit = Ratelimiter::builder(1, Duration::from_millis(200)).build()?;
    while let Some(item) = stream.next().await {
        let chunk = item?;
        let len = chunk.len();
        file.write_all(&chunk).await?;
        downloaded += len;

        let percent = if content_length > 0 {
            (downloaded as f64 / content_length as f64) * 100.0
        } else {
            0.0
        };

        if let Ok(_) = limit.try_wait() {
            let _ = service_tx.send(Normal(format!("下载中... {:.2}%", percent)));
        }
    }

    Ok(())
}