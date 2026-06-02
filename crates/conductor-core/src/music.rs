use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MusicInfo {
    pub state: PlaybackState,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<u32>,
    pub position: Option<u32>,
    pub timestamp: DateTime<Utc>,
}

impl Default for MusicInfo {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            title: None,
            artist: None,
            album: None,
            duration: None,
            position: None,
            timestamp: Utc::now(),
        }
    }
}

pub fn get_current_music_info() -> anyhow::Result<MusicInfo> {
    Ok(MusicInfo::default())
}

pub async fn poll_music_state() -> anyhow::Result<MusicInfo> {
    get_current_music_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_music_info_default() {
        let info = MusicInfo::default();
        assert_eq!(info.state, PlaybackState::Stopped);
        assert!(info.title.is_none());
    }

    #[test]
    fn test_playback_state_enum() {
        let states = [
            PlaybackState::Playing,
            PlaybackState::Paused,
            PlaybackState::Stopped,
        ];
        assert!(states.len() == 3);
    }
}
