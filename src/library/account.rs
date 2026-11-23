//! Copying a YouTube account's public subscriptions and playlists into TubeGlass, without signing
//! in, and keeping them in step: a look at the public pages of the account's channel.

use crate::*;
use crate::yt::{Channel, ChannelHit, PlaylistHit, TabData};

/// What an account's public pages show.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// the account's own channel
    pub source: Option<Channel>,
    pub channels: Vec<ChannelHit>,
    pub playlists: Vec<PlaylistHit>,
}

/// Reads a channel (link, `@handle` or id): its public channels and its public playlists.
pub fn snapshot(input: &str) -> Result<Snapshot, String> {
    let source = yt::resolve_channel(input.trim())?;
    let channels = yt::public_channels(&source).unwrap_or_default();
    let mut playlists: Vec<PlaylistHit> = vec![];
    if let Ok((_, TabData::Playlists(first), mut next)) = yt::channel_tab(&source, "playlists") {
        playlists = first;
        let mut pages = 0;
        while let (Some(t), true) = (next.take(), pages < 8) {
            pages += 1;
            match yt::channel_more(&source, "playlists", &t) {
                Ok((TabData::Playlists(more), n)) => {
                    playlists.extend(more);
                    next = n;
                }
                _ => break,
            }
        }
    }
    let mut seen = HashSet::new();
    playlists.retain(|p| seen.insert(p.id.clone()));
    Ok(Snapshot { source: Some(source), channels, playlists })
}

/// What a re-check of the source adds. Nothing is ever removed.
#[derive(Debug, Default, PartialEq)]
pub struct SyncPlan {
    /// channels not in the library and not seen by an earlier sync
    pub new_channels: Vec<Channel>,
    /// playlists of the source that are not imported yet (only counted, never added on their own)
    pub new_playlists: usize,
    /// imported playlists to look into for new items: (source playlist id, local list id)
    pub check_lists: Vec<(String, String)>,
}

/// Works out what a sync would add. `seen` holds everything an earlier import or sync already
/// considered (`ch:<id>`, `pl:<playlist>:<video>`), so what the user removed stays removed.
pub fn plan(snap: &Snapshot, have: &[Channel], seen: &HashSet<String>, imported: &HashMap<String, String>) -> SyncPlan {
    let new_channels = snap
        .channels
        .iter()
        .filter(|h| !have.iter().any(|c| c.id == h.channel.id) && !seen.contains(&format!("ch:{}", h.channel.id)))
        .map(|h| h.channel.clone())
        .collect();
    let new_playlists = snap.playlists.iter().filter(|p| !imported.contains_key(&p.id)).count();
    let check_lists = snap.playlists.iter().filter_map(|p| imported.get(&p.id).map(|l| (p.id.clone(), l.clone()))).collect();
    SyncPlan { new_channels, new_playlists, check_lists }
}

/// Videos of `fetched` that a local list does not have yet and that no earlier sync has seen.
pub fn new_items(playlist: &str, fetched: &[Video], in_list: &[Video], seen: &HashSet<String>) -> Vec<Video> {
    fetched
        .iter()
        .filter(|v| !in_list.iter().any(|x| x.id == v.id) && !seen.contains(&format!("pl:{playlist}:{}", v.id)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str) -> ChannelHit {
        ChannelHit { channel: Channel { id: id.into(), name: id.into(), avatar: String::new() }, handle: String::new(), subs: String::new() }
    }
    fn video(id: &str) -> Video {
        Video { id: id.into(), title: String::new(), thumb: String::new(), duration: String::new(), meta: String::new(), kind: Kind::Video, channel_id: String::new(), channel_name: String::new(), rank: 0, short: false, music: false }
    }

    #[test]
    fn a_sync_adds_only_what_is_new_and_never_what_was_removed() {
        let snap = Snapshot {
            source: None,
            channels: vec![hit("A"), hit("B"), hit("C")],
            playlists: vec![
                PlaylistHit { id: "P1".into(), title: String::new(), thumb: String::new(), owner: String::new(), count: String::new() },
                PlaylistHit { id: "P2".into(), title: String::new(), thumb: String::new(), owner: String::new(), count: String::new() },
            ],
        };
        let have = vec![Channel { id: "A".into(), name: "A".into(), avatar: String::new() }];
        // B was seen by an earlier sync and then unsubscribed by the user: it stays away
        let seen: HashSet<String> = ["ch:B".to_string()].into();
        let imported: HashMap<String, String> = [("P1".to_string(), "pl-1".to_string())].into();
        let p = plan(&snap, &have, &seen, &imported);
        assert_eq!(p.new_channels.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["C"]);
        assert_eq!(p.new_playlists, 1);
        assert_eq!(p.check_lists, vec![("P1".to_string(), "pl-1".to_string())]);
    }

    #[test]
    fn new_playlist_items_skip_what_the_list_has_or_the_user_removed() {
        let fetched = vec![video("a"), video("b"), video("c")];
        let in_list = vec![video("a")];
        let seen: HashSet<String> = ["pl:P:b".to_string()].into();
        let n = new_items("P", &fetched, &in_list, &seen);
        assert_eq!(n.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
    }

    #[test]
    fn channels_come_out_of_a_grid() {
        let v: serde_json::Value = serde_json::json!({
            "x": [{ "gridChannelRenderer": {
                "channelId": "UCabc", "title": { "simpleText": "Studio" },
                "thumbnail": { "thumbnails": [{ "url": "//img/1" }, { "url": "//img/2" }] },
                "subscriberCountText": { "simpleText": "1M subscribers" },
                "navigationEndpoint": { "browseEndpoint": { "canonicalBaseUrl": "/@studio" } }
            } }]
        });
        let mut out = vec![];
        yt::collect_grid_channels(&v, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].channel.avatar, "https://img/2");
        assert_eq!(out[0].handle, "@studio");
        assert_eq!(out[0].subs, "1M subscribers");
    }
}

#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore = "needs network"]
    fn a_real_channel_can_be_read() {
        let s = snapshot("@mkbhd").unwrap();
        println!("ACCT source={:?} channels={} playlists={}", s.source.map(|c| c.name), s.channels.len(), s.playlists.len());
        println!("ACCT first channels: {:?}", s.channels.iter().take(3).map(|h| (&h.channel.name, &h.handle)).collect::<Vec<_>>());
        println!("ACCT first playlists: {:?}", s.playlists.iter().take(3).map(|p| (&p.title, &p.count)).collect::<Vec<_>>());
        assert!(!s.playlists.is_empty());
    }
}
