use crate::core::statistics::GachaStatistics;

pub(crate) enum MessageType {
    Normal(String),
    Warning(String),
    Gacha((String, GachaStatistics)),
    Player(Vec<String>),
    Update(bool, String),
}