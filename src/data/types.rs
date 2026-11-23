//! Types shared by the whole app: pages, tabs, queue items, background messages.

use crate::*;

/// One entry of the play queue.
#[derive(Clone)]
pub(crate) enum QItem {
    Yt(Video),
    Dl(DlItem),
}

impl QItem {
    /// Stable text id of a queue entry (used to avoid duplicates).
    pub(crate) fn key(&self) -> String {
        match self {
            QItem::Yt(v) => v.id.clone(),
            QItem::Dl(d) => format!("dl:{}:{}", d.id, d.audio_only),
        }
    }
    /// Title to show for the queue entry.
    pub(crate) fn title(&self) -> String {
        match self {
            QItem::Yt(v) => v.title.clone(),
            QItem::Dl(d) => d.title.clone(),
        }
    }
    /// Second line of the queue entry (channel, or "downloaded").
    pub(crate) fn subtitle(&self) -> String {
        match self {
            QItem::Yt(v) => v.channel_name.clone(),
            QItem::Dl(d) => format!("{} · downloaded", d.channel),
        }
    }
}

/// The tabs of the panel beside the player.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SideTab {
    Comments,
    Lyrics,
    Queue,
    Related,
    Transcript,
    Notes,
    Clip,
}

/// Position of a channel tab (saved with the window state).
pub(crate) fn tab_index(t: ChanTab) -> usize {
    match t {
        ChanTab::Videos => 0,
        ChanTab::Live => 1,
        ChanTab::Shorts => 2,
        ChanTab::Playlists => 3,
        ChanTab::About => 4,
    }
}

/// The channel tab at a saved position.
pub(crate) fn tab_from_index(i: usize) -> ChanTab {
    [ChanTab::Videos, ChanTab::Live, ChanTab::Shorts, ChanTab::Playlists, ChanTab::About]
        .get(i)
        .copied()
        .unwrap_or(ChanTab::Videos)
}

/// Title shown while a video's real title is still being fetched.
pub(crate) const PLACEHOLDER_TITLE: &str = "Loading…";

/// What a pasted address in the search box turned out to be.
pub(crate) enum Link {
    Playlist(String),
    Video(String),
    None,
}

/// Recognises pasted YouTube playlist / video / shorts links (and bare 11-character ids).
pub(crate) fn classify_link(q: &str) -> Link {
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let param = |name: &str| -> Option<String> {
        let key = format!("{name}=");
        let i = q.find(&format!("?{key}")).or_else(|| q.find(&format!("&{key}")))? + key.len() + 1;
        let v: String = q[i..].chars().take_while(|c| *c != '&' && *c != '#').collect();
        ok(&v).then_some(v)
    };
    if !q.contains("youtube.com") && !q.contains("youtu.be") {
        return Link::None;
    }
    if q.contains("/playlist") {
        if let Some(l) = param("list") {
            return Link::Playlist(l);
        }
    }
    if let Some(v) = param("v") {
        return Link::Video(v);
    }
    for marker in ["youtu.be/", "/shorts/", "/live/", "/embed/"] {
        if let Some(i) = q.find(marker) {
            let v: String = q[i + marker.len()..].chars().take_while(|c| *c != '?' && *c != '&' && *c != '#' && *c != '/').collect();
            if v.len() == 11 && ok(&v) {
                return Link::Video(v);
            }
        }
    }
    if let Some(l) = param("list") {
        return Link::Playlist(l);
    }
    Link::None
}

/// The sleep timer: pause at a time, or after the current item.
pub(crate) enum SleepMode {
    At(Instant),
    EndOfVideo,
}

/// Lists the app creates itself and that can't be deleted.
/// Local files and podcast episodes: no YouTube page, comments or captions behind them.
pub(crate) fn non_yt(id: &str) -> bool {
    id.starts_with("local:") || id.starts_with("pod:")
}

/// True for Favorites, Watch later and Liked songs, which cannot be deleted.
pub(crate) fn is_builtin_list(id: &str) -> bool {
    id == "favorites" || id == "watchlater" || id == "liked"
}

/// What the right-click menu of a video card can ask for.
pub(crate) enum CardAction {
    RemoveFromHistory,
    Download(bool, u32),
    ToggleList(String),
    NewList,
    RemoveFromList,
    PlayAudio,
    PlayNext,
    AddToQueue,
    NotInterested,
    FewerFromChannel,
}

/// What a video card needs to know to draw itself (badges, saved state, lists).
pub(crate) struct CardInfo<'a> {
    pub(crate) is_new: bool,
    /// fraction of the video already watched
    pub(crate) watched: Option<f32>,
    pub(crate) in_history_view: bool,
    pub(crate) progress: Option<f32>,
    pub(crate) video_saved: bool,
    pub(crate) audio_saved: bool,
    pub(crate) lists: &'a [LocalList],
    pub(crate) current_list: Option<&'a str>,
    /// list layout: thumbnail on the left, text on the right
    pub(crate) row: bool,
}

/// Whether the sidebar is collapsed (read by the sidebar rows while drawing).
pub(crate) static SIDE_COLLAPSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The tabs of a channel page.
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum ChanTab {
    Videos,
    Live,
    Shorts,
    Playlists,
    About,
}

impl ChanTab {
    /// Short name of the channel tab, used in cache keys and requests.
    pub(crate) fn key(self) -> &'static str {
        match self {
            ChanTab::Videos => "videos",
            ChanTab::Live => "streams",
            ChanTab::Shorts => "shorts",
            ChanTab::Playlists => "playlists",
            ChanTab::About => "about",
        }
    }
}

/// Results sent from background threads to the interface thread.
pub(crate) enum Msg {
    Resolved(Result<Channel, String>),
    Feed(String, Result<Vec<Video>, String>),
    Streams(String, Result<Streams, String>),
    SearchVideos(String, Result<Vec<Video>, String>),
    SearchChannels(String, Result<Vec<ChannelHit>, String>),
    SearchPlaylists(String, Result<Vec<PlaylistHit>, String>),
    Suggestions(String, Vec<String>),
    Playlist(String, Result<(String, Vec<Video>), String>),
    /// A playlist imported from a pasted link: its address and the title and videos found.
    ListImported(String, Result<(String, Vec<Video>), String>),
    Live(Result<Vec<Video>, String>),
    ChanTab(String, String, Result<(ChannelHeader, TabData, Option<String>), String>),
    ChanMore(String, String, Result<(TabData, Option<String>), String>),
    ChanSearch(String, String, Result<(Vec<Video>, Vec<PlaylistHit>), String>),
    DlProgress(String, f32, u64),
    LocalProbe(String, Result<(Streams, String, String), String>),
    Picked(Option<String>),
    DirPicked(Option<String>),
    Imported(Vec<subs::Imported>),
    ExportedSubs(usize),
    Online(bool),
    /// Whether ffmpeg could be started at launch.
    FfmpegChecked(bool),
    FfmpegProgress(f32),
    FfmpegInstalled(Result<std::path::PathBuf, String>),
    /// Result of looking for a newer release (`true` = the user asked for it).
    UpdateChecked(Result<Option<update::Release>, String>, bool),
    UpdateProgress(f32),
    UpdateDownloaded(Result<std::path::PathBuf, String>),
    Podcast(String, bool, Result<(String, podcasts::Show, Vec<podcasts::Episode>), String>),
    PodSearch(String, Result<Vec<podcasts::Hit>, String>),
    Transcript(String, Result<Vec<yt::Cue>, String>),
    ClipProgress(f32),
    ClipDone(Result<std::path::PathBuf, String>),
    Storage(StorageInfo),
    Note(String),
    Diag(Vec<(String, bool, String)>),
    Restored,
    OpenChannel(Result<Channel, String>),
    LiveCheck(Vec<(String, Video)>),
    MusicHome(Result<ytmusic::Page, String>),
    MusicSugg(String, Vec<String>),
    Lyrics(String, Result<ytmusic::Lyrics, String>),
    ForYou(Vec<(String, Vec<ytmusic::Card>)>),
    MusicSearch(String, ytmusic::SearchKind, Result<ytmusic::Results, String>),
    MusicPage(String, Result<ytmusic::Page, String>),
    Cues(String, usize, Result<Vec<yt::Cue>, String>),
    /// The second caption track: the key it was asked for (video|language) and the cues.
    Cues2(String, Result<Vec<yt::Cue>, String>),
    /// The look-up of an account in the import dialog.
    AcctLooked(Result<account::Snapshot, String>),
    /// A background re-check of the synced account.
    AcctSynced(Result<account::Snapshot, String>),
    /// Videos of an imported playlist fetched for a sync: (source playlist id, local list id, result).
    SyncList(String, String, Result<(String, Vec<Video>), String>),
    /// The text of a TubeGlass backup picked for "copy settings from a backup".
    BackupPicked(Option<String>),
    Sponsor(String, Result<Vec<yt::SkipSeg>, String>),
    Related(String, Result<Vec<Video>, String>),
    ChannelInfo(String, Channel),
    DlDone(String, Result<DlItem, String>),
    Comments(String, bool, Result<yt::CommentsPage, String>),
    Replies(String, String, bool, Result<(Vec<Comment>, Option<String>), String>),
}

/// The page that is open.
#[derive(PartialEq, Clone)]
pub(crate) enum Sel {
    Home,
    Music,
    MusicAlbum(String),
    MusicArtist(String),
    Group(String),
    Settings,
    History,
    Stats,
    Clips,
    Podcasts,
    Podcast(String),
    Local(String),
    Downloads,
    Live,
    Playlist(String),
    Search,
    Channel(String),
}

/// The All / New / Live / Videos chips above a video list.
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Filter {
    All,
    New,
    Live,
    Videos,
}

/// State of a search inside one channel.
pub(crate) struct ChanSearch {
    pub(crate) chan: String,
    pub(crate) query: String,
    pub(crate) res: Option<Result<(Vec<Video>, Vec<PlaylistHit>), String>>,
}

/// The replies of one comment as they are loaded.
#[derive(Default)]
pub(crate) struct RepliesState {
    pub(crate) open: bool,
    pub(crate) loading: bool,
    pub(crate) items: Vec<Comment>,
    pub(crate) next: Option<String>,
    pub(crate) err: Option<String>,
}

/// One entry of the back-navigation history.
#[derive(Clone)]
pub(crate) struct NavEntry {
    pub(crate) sel: Sel,
    pub(crate) tab: ChanTab,
    pub(crate) query: String,
}

/// A running download as the Downloads page shows it.
pub(crate) struct DlJob {
    pub(crate) id: String,
    pub(crate) vid: String,
    pub(crate) pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) bytes: u64,
    pub(crate) started: Instant,
    /// time spent paused, so speed / ETA only count the time actually downloading
    pub(crate) idle: Duration,
    pub(crate) paused_at: Option<Instant>,
    pub(crate) title: String,
    pub(crate) thumb: String,
    pub(crate) progress: f32,
    pub(crate) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// what to start again if the transfer fails
    pub(crate) retry: (Video, bool, u32, String),
}
