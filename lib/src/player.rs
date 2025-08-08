#![allow(clippy::module_name_repetitions)]
use anyhow::{Context, anyhow, bail};

// using lower mod to restrict clippy
#[allow(clippy::pedantic)]
mod protobuf {
    tonic::include_proto!("player");
}

pub use protobuf::*;

use crate::config::v2::server::LoopMode;

// implement transform function for easy use
impl From<protobuf::Duration> for std::time::Duration {
    fn from(value: protobuf::Duration) -> Self {
        std::time::Duration::new(value.secs, value.nanos)
    }
}

impl From<std::time::Duration> for protobuf::Duration {
    fn from(value: std::time::Duration) -> Self {
        Self {
            secs: value.as_secs(),
            nanos: value.subsec_nanos(),
        }
    }
}

/// The primitive in which time (current position / total duration) will be stored as
pub type PlayerTimeUnit = std::time::Duration;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum RunningStatus {
    #[default]
    Stopped,
    Running,
    Paused,
}

impl RunningStatus {
    #[must_use]
    pub fn as_u32(&self) -> u32 {
        match self {
            RunningStatus::Stopped => 0,
            RunningStatus::Running => 1,
            RunningStatus::Paused => 2,
        }
    }

    #[must_use]
    pub fn from_u32(status: u32) -> Self {
        match status {
            1 => RunningStatus::Running,
            2 => RunningStatus::Paused,
            _ => RunningStatus::Stopped,
        }
    }
}

impl std::fmt::Display for RunningStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "Running"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Paused => write!(f, "Paused"),
        }
    }
}

/// Struct to keep both values with a name, as tuples cannot have named fields
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerProgress {
    pub position: Option<PlayerTimeUnit>,
    /// Total duration of the currently playing track, if there is a known total duration
    pub total_duration: Option<PlayerTimeUnit>,
}

impl From<protobuf::PlayerTime> for PlayerProgress {
    fn from(value: protobuf::PlayerTime) -> Self {
        Self {
            position: value.position.map(Into::into),
            total_duration: value.total_duration.map(Into::into),
        }
    }
}

impl From<PlayerProgress> for protobuf::PlayerTime {
    fn from(value: PlayerProgress) -> Self {
        Self {
            position: value.position.map(Into::into),
            total_duration: value.total_duration.map(Into::into),
        }
    }
}

impl TryFrom<protobuf::UpdateProgress> for PlayerProgress {
    type Error = anyhow::Error;

    fn try_from(value: protobuf::UpdateProgress) -> Result<Self, Self::Error> {
        let Some(val) = value.progress else {
            bail!("Expected \"UpdateProgress\" to contain \"Some(progress)\"");
        };

        Ok(Self::from(val))
    }
}

impl From<PlayerProgress> for protobuf::UpdateProgress {
    fn from(value: PlayerProgress) -> Self {
        Self {
            progress: Some(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackChangedInfo {
    /// Current track index in the playlist
    pub current_track_index: u64,
    /// Indicate if the track changed to another track
    pub current_track_updated: bool,
    /// Title of the current track / radio
    pub title: Option<String>,
    /// Current progress of the track
    pub progress: Option<PlayerProgress>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateEvents {
    MissedEvents { amount: u64 },
    VolumeChanged { volume: u16 },
    SpeedChanged { speed: i32 },
    PlayStateChanged { playing: u32 },
    TrackChanged(TrackChangedInfo),
    GaplessChanged { gapless: bool },
    PlaylistChanged(UpdatePlaylistEvents),
    Progress(PlayerProgress),
}

type StreamTypes = protobuf::stream_updates::Type;

// mainly for server to grpc
impl From<UpdateEvents> for protobuf::StreamUpdates {
    fn from(value: UpdateEvents) -> Self {
        let val = match value {
            UpdateEvents::MissedEvents { amount } => {
                StreamTypes::MissedEvents(UpdateMissedEvents { amount })
            }
            UpdateEvents::VolumeChanged { volume } => {
                StreamTypes::VolumeChanged(UpdateVolumeChanged {
                    msg: Some(VolumeReply {
                        volume: u32::from(volume),
                    }),
                })
            }
            UpdateEvents::SpeedChanged { speed } => StreamTypes::SpeedChanged(UpdateSpeedChanged {
                msg: Some(SpeedReply { speed }),
            }),
            UpdateEvents::PlayStateChanged { playing } => {
                StreamTypes::PlayStateChanged(UpdatePlayStateChanged {
                    msg: Some(PlayState { status: playing }),
                })
            }
            UpdateEvents::TrackChanged(info) => StreamTypes::TrackChanged(UpdateTrackChanged {
                current_track_index: info.current_track_index,
                current_track_updated: info.current_track_updated,
                optional_title: info
                    .title
                    .map(protobuf::update_track_changed::OptionalTitle::Title),
                progress: info.progress.map(Into::into),
            }),
            UpdateEvents::GaplessChanged { gapless } => {
                StreamTypes::GaplessChanged(UpdateGaplessChanged {
                    msg: Some(GaplessState { gapless }),
                })
            }
            UpdateEvents::PlaylistChanged(ev) => StreamTypes::PlaylistChanged(ev.into()),
            UpdateEvents::Progress(ev) => StreamTypes::ProgressChanged(ev.into()),
        };

        Self { r#type: Some(val) }
    }
}

// mainly for grpc to client(tui)
impl TryFrom<protobuf::StreamUpdates> for UpdateEvents {
    type Error = anyhow::Error;

    fn try_from(value: protobuf::StreamUpdates) -> Result<Self, Self::Error> {
        let value = unwrap_msg(value.r#type, "StreamUpdates.type")?;

        let res = match value {
            StreamTypes::VolumeChanged(ev) => Self::VolumeChanged {
                volume: clamp_u16(
                    unwrap_msg(ev.msg, "StreamUpdates.types.volume_changed.msg")?.volume,
                ),
            },
            StreamTypes::SpeedChanged(ev) => Self::SpeedChanged {
                speed: unwrap_msg(ev.msg, "StreamUpdates.types.speed_changed.msg")?.speed,
            },
            StreamTypes::PlayStateChanged(ev) => Self::PlayStateChanged {
                playing: unwrap_msg(ev.msg, "StreamUpdates.types.play_state_changed.msg")?.status,
            },
            StreamTypes::MissedEvents(ev) => Self::MissedEvents { amount: ev.amount },
            StreamTypes::TrackChanged(ev) => Self::TrackChanged(TrackChangedInfo {
                current_track_index: ev.current_track_index,
                current_track_updated: ev.current_track_updated,
                title: ev.optional_title.map(|v| {
                    let protobuf::update_track_changed::OptionalTitle::Title(v) = v;
                    v
                }),
                progress: ev.progress.map(Into::into),
            }),
            StreamTypes::GaplessChanged(ev) => Self::GaplessChanged {
                gapless: unwrap_msg(ev.msg, "StreamUpdates.types.gapless_changed.msg")?.gapless,
            },
            StreamTypes::PlaylistChanged(ev) => Self::PlaylistChanged(
                ev.try_into()
                    .context("In \"StreamUpdates.types.playlist_changed\"")?,
            ),
            StreamTypes::ProgressChanged(ev) => Self::Progress(
                ev.try_into()
                    .context("In \"StreamUpdates.types.progress_changed\"")?,
            ),
        };

        Ok(res)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistAddTrackInfo {
    /// The Index at which a track was added at.
    /// If this is not at the end, all tracks at this index and beyond should be shifted.
    pub at_index: u64,
    pub title: Option<String>,
    /// Duration of the track
    pub duration: PlayerTimeUnit,
    pub trackid: playlist_helpers::PlaylistTrackSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistRemoveTrackInfo {
    /// The Index at which a track was removed at.
    pub at_index: u64,
    /// The Id of the removed track.
    pub trackid: playlist_helpers::PlaylistTrackSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistLoopModeInfo {
    /// The actual mode, mapped to [`LoopMode`]
    pub mode: u32,
}

impl From<LoopMode> for PlaylistLoopModeInfo {
    fn from(value: LoopMode) -> Self {
        Self {
            mode: u32::from(value.discriminant()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistSwapInfo {
    pub index_a: u64,
    pub index_b: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistShuffledInfo {
    pub tracks: PlaylistTracks,
}

/// Separate nested enum to handle all playlist related events
#[derive(Debug, Clone, PartialEq)]
pub enum UpdatePlaylistEvents {
    PlaylistAddTrack(PlaylistAddTrackInfo),
    PlaylistRemoveTrack(PlaylistRemoveTrackInfo),
    PlaylistCleared,
    PlaylistLoopMode(PlaylistLoopModeInfo),
    PlaylistSwapTracks(PlaylistSwapInfo),
    PlaylistShuffled(PlaylistShuffledInfo),
}

type PPlaylistTypes = protobuf::update_playlist::Type;

// mainly for server to grpc
impl From<UpdatePlaylistEvents> for protobuf::UpdatePlaylist {
    fn from(value: UpdatePlaylistEvents) -> Self {
        let val = match value {
            UpdatePlaylistEvents::PlaylistAddTrack(vals) => {
                PPlaylistTypes::AddTrack(protobuf::PlaylistAddTrack {
                    at_index: vals.at_index,
                    optional_title: vals
                        .title
                        .map(protobuf::playlist_add_track::OptionalTitle::Title),
                    duration: Some(vals.duration.into()),
                    id: Some(vals.trackid.into()),
                })
            }
            UpdatePlaylistEvents::PlaylistRemoveTrack(vals) => {
                PPlaylistTypes::RemoveTrack(protobuf::PlaylistRemoveTrack {
                    at_index: vals.at_index,
                    id: Some(vals.trackid.into()),
                })
            }
            UpdatePlaylistEvents::PlaylistCleared => PPlaylistTypes::Cleared(PlaylistCleared {}),
            UpdatePlaylistEvents::PlaylistLoopMode(vals) => {
                PPlaylistTypes::LoopMode(PlaylistLoopMode { mode: vals.mode })
            }
            UpdatePlaylistEvents::PlaylistSwapTracks(vals) => {
                PPlaylistTypes::SwapTracks(protobuf::PlaylistSwapTracks {
                    index_a: vals.index_a,
                    index_b: vals.index_b,
                })
            }
            UpdatePlaylistEvents::PlaylistShuffled(vals) => {
                PPlaylistTypes::Shuffled(protobuf::PlaylistShuffled {
                    shuffled: Some(vals.tracks),
                })
            }
        };

        Self { r#type: Some(val) }
    }
}

// mainly for grpc to client(tui)
impl TryFrom<protobuf::UpdatePlaylist> for UpdatePlaylistEvents {
    type Error = anyhow::Error;

    fn try_from(value: protobuf::UpdatePlaylist) -> Result<Self, Self::Error> {
        let value = unwrap_msg(value.r#type, "UpdatePlaylist.type")?;

        let res = match value {
            PPlaylistTypes::AddTrack(ev) => Self::PlaylistAddTrack(PlaylistAddTrackInfo {
                at_index: ev.at_index,
                title: ev.optional_title.map(|v| {
                    let protobuf::playlist_add_track::OptionalTitle::Title(v) = v;
                    v
                }),
                duration: unwrap_msg(ev.duration, "UpdatePlaylist.type.add_track.duration")?.into(),
                trackid: unwrap_msg(
                    unwrap_msg(ev.id, "UpdatePlaylist.type.add_track.id")?.source,
                    "UpdatePlaylist.type.add_track.id.source",
                )?
                .try_into()?,
            }),
            PPlaylistTypes::RemoveTrack(ev) => Self::PlaylistRemoveTrack(PlaylistRemoveTrackInfo {
                at_index: ev.at_index,
                trackid: unwrap_msg(
                    unwrap_msg(ev.id, "UpdatePlaylist.type.remove_track.id")?.source,
                    "UpdatePlaylist.type.remove_track.id.source",
                )?
                .try_into()?,
            }),
            PPlaylistTypes::Cleared(_) => Self::PlaylistCleared,
            PPlaylistTypes::LoopMode(ev) => {
                Self::PlaylistLoopMode(PlaylistLoopModeInfo { mode: ev.mode })
            }
            PPlaylistTypes::SwapTracks(ev) => Self::PlaylistSwapTracks(PlaylistSwapInfo {
                index_a: ev.index_a,
                index_b: ev.index_b,
            }),
            PPlaylistTypes::Shuffled(ev) => {
                let shuffled = unwrap_msg(ev.shuffled, "UpdatePlaylist.type.shuffled.shuffled")?;
                Self::PlaylistShuffled(PlaylistShuffledInfo { tracks: shuffled })
            }
        };

        Ok(res)
    }
}

/// Easily unwrap a given grpc option and covert it to a result, with a location on None
fn unwrap_msg<T>(opt: Option<T>, place: &str) -> Result<T, anyhow::Error> {
    match opt {
        Some(val) => Ok(val),
        None => Err(anyhow!("Got \"None\" in grpc \"{place}\"!")),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn clamp_u16(val: u32) -> u16 {
    val.min(u32::from(u16::MAX)) as u16
}

pub mod playlist_helpers {
    use anyhow::Context;

    use super::{PlaylistTracksToRemoveClear, protobuf, unwrap_msg};

    /// A Id / Source for a given Track
    #[derive(Debug, Clone, PartialEq)]
    pub enum PlaylistTrackSource {
        Path(String),
        Url(String),
        PodcastUrl(String),
    }

    impl From<PlaylistTrackSource> for protobuf::track_id::Source {
        fn from(value: PlaylistTrackSource) -> Self {
            match value {
                PlaylistTrackSource::Path(v) => Self::Path(v),
                PlaylistTrackSource::Url(v) => Self::Url(v),
                PlaylistTrackSource::PodcastUrl(v) => Self::PodcastUrl(v),
            }
        }
    }

    impl From<PlaylistTrackSource> for protobuf::TrackId {
        fn from(value: PlaylistTrackSource) -> Self {
            Self {
                source: Some(value.into()),
            }
        }
    }

    impl TryFrom<protobuf::track_id::Source> for PlaylistTrackSource {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::track_id::Source) -> Result<Self, Self::Error> {
            Ok(match value {
                protobuf::track_id::Source::Path(v) => Self::Path(v),
                protobuf::track_id::Source::Url(v) => Self::Url(v),
                protobuf::track_id::Source::PodcastUrl(v) => Self::PodcastUrl(v),
            })
        }
    }

    impl TryFrom<protobuf::TrackId> for PlaylistTrackSource {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::TrackId) -> Result<Self, Self::Error> {
            unwrap_msg(value.source, "TrackId.source").and_then(Self::try_from)
        }
    }

    /// Data for requesting some tracks to be added in the server
    #[derive(Debug, Clone, PartialEq)]
    pub struct PlaylistAddTrack {
        pub at_index: u64,
        pub tracks: Vec<PlaylistTrackSource>,
    }

    impl PlaylistAddTrack {
        #[must_use]
        pub fn new_single(at_index: u64, track: PlaylistTrackSource) -> Self {
            Self {
                at_index,
                tracks: vec![track],
            }
        }

        #[must_use]
        pub fn new_vec(at_index: u64, tracks: Vec<PlaylistTrackSource>) -> Self {
            Self { at_index, tracks }
        }
    }

    impl From<PlaylistAddTrack> for protobuf::PlaylistTracksToAdd {
        fn from(value: PlaylistAddTrack) -> Self {
            Self {
                at_index: value.at_index,
                tracks: value.tracks.into_iter().map(Into::into).collect(),
            }
        }
    }

    impl TryFrom<protobuf::PlaylistTracksToAdd> for PlaylistAddTrack {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::PlaylistTracksToAdd) -> Result<Self, Self::Error> {
            let tracks = value
                .tracks
                .into_iter()
                .map(|v| PlaylistTrackSource::try_from(v).context("PlaylistTracksToAdd.tracks"))
                .collect::<Result<Vec<_>, anyhow::Error>>()?;

            Ok(Self {
                at_index: value.at_index,
                tracks,
            })
        }
    }

    /// Data for requesting some tracks to be removed in the server
    #[derive(Debug, Clone, PartialEq)]
    pub struct PlaylistRemoveTrackIndexed {
        pub at_index: u64,
        pub tracks: Vec<PlaylistTrackSource>,
    }

    impl PlaylistRemoveTrackIndexed {
        #[must_use]
        pub fn new_single(at_index: u64, track: PlaylistTrackSource) -> Self {
            Self {
                at_index,
                tracks: vec![track],
            }
        }

        #[must_use]
        pub fn new_vec(at_index: u64, tracks: Vec<PlaylistTrackSource>) -> Self {
            Self { at_index, tracks }
        }
    }

    impl From<PlaylistRemoveTrackIndexed> for protobuf::PlaylistTracksToRemoveIndexed {
        fn from(value: PlaylistRemoveTrackIndexed) -> Self {
            Self {
                at_index: value.at_index,
                tracks: value.tracks.into_iter().map(Into::into).collect(),
            }
        }
    }

    impl TryFrom<protobuf::PlaylistTracksToRemoveIndexed> for PlaylistRemoveTrackIndexed {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::PlaylistTracksToRemoveIndexed) -> Result<Self, Self::Error> {
            let tracks = value
                .tracks
                .into_iter()
                .map(|v| {
                    PlaylistTrackSource::try_from(v).context("PlaylistTracksToRemoveIndexed.tracks")
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()?;

            Ok(Self {
                at_index: value.at_index,
                tracks,
            })
        }
    }

    /// Data for requesting some tracks to be removed in the server
    #[derive(Debug, Clone, PartialEq)]
    pub enum PlaylistRemoveTrackType {
        Indexed(PlaylistRemoveTrackIndexed),
        Clear,
    }

    type PToRemoveTypes = protobuf::playlist_tracks_to_remove::Type;

    impl From<PlaylistRemoveTrackType> for protobuf::PlaylistTracksToRemove {
        fn from(value: PlaylistRemoveTrackType) -> Self {
            Self {
                r#type: Some(match value {
                    PlaylistRemoveTrackType::Indexed(v) => PToRemoveTypes::Indexed(v.into()),
                    PlaylistRemoveTrackType::Clear => {
                        PToRemoveTypes::Clear(PlaylistTracksToRemoveClear {})
                    }
                }),
            }
        }
    }

    impl TryFrom<protobuf::PlaylistTracksToRemove> for PlaylistRemoveTrackType {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::PlaylistTracksToRemove) -> Result<Self, Self::Error> {
            let value = unwrap_msg(value.r#type, "PlaylistTracksToRemove.type")?;

            Ok(match value {
                PToRemoveTypes::Indexed(v) => Self::Indexed(v.try_into()?),
                PToRemoveTypes::Clear(_) => Self::Clear,
            })
        }
    }

    /// Data for requesting some tracks to be swapped in the server
    #[derive(Debug, Clone, PartialEq)]
    pub struct PlaylistSwapTrack {
        pub index_a: u64,
        pub index_b: u64,
    }

    impl From<PlaylistSwapTrack> for protobuf::PlaylistSwapTracks {
        fn from(value: PlaylistSwapTrack) -> Self {
            Self {
                index_a: value.index_a,
                index_b: value.index_b,
            }
        }
    }

    impl TryFrom<protobuf::PlaylistSwapTracks> for PlaylistSwapTrack {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::PlaylistSwapTracks) -> Result<Self, Self::Error> {
            Ok(Self {
                index_a: value.index_a,
                index_b: value.index_b,
            })
        }
    }

    /// Data for requesting to skip / play a specific track
    #[derive(Debug, Clone, PartialEq)]
    pub struct PlaylistPlaySpecific {
        pub track_index: u64,
        pub id: PlaylistTrackSource,
    }

    impl From<PlaylistPlaySpecific> for protobuf::PlaylistPlaySpecific {
        fn from(value: PlaylistPlaySpecific) -> Self {
            Self {
                track_index: value.track_index,
                id: Some(value.id.into()),
            }
        }
    }

    impl TryFrom<protobuf::PlaylistPlaySpecific> for PlaylistPlaySpecific {
        type Error = anyhow::Error;

        fn try_from(value: protobuf::PlaylistPlaySpecific) -> Result<Self, Self::Error> {
            Ok(Self {
                track_index: value.track_index,
                id: unwrap_msg(value.id, "PlaylistPlaySpecific.id").and_then(|v| {
                    PlaylistTrackSource::try_from(v).context("PlaylistPlaySpecific.id")
                })?,
            })
        }
    }
}
