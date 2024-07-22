use crate::core::statistics::GachaStatistics;
use crate::core::update::Release;

pub(crate) enum MessageType {
    CheckUpdate,
    NeedUpdate(Release),
    DownloadFile(Release, String),
    Normal(String),
    Warning(String),
    Gacha((String, GachaStatistics)),
    Player(Vec<String>),
    UpdateData(bool, String),
}