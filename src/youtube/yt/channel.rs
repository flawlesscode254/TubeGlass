//! Channels: resolving handles, the feed, the Videos / Live / Shorts / Playlists tabs, in-channel search and live streams.

use super::*;

/// Turns a handle, link or id into a channel page address.
pub(crate) fn channel_url(input: &str) -> Res<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return Err("type a channel name or @handle".into());
    }
    if s.contains("youtube.com/") {
        let after = s.split("youtube.com/").nth(1).unwrap_or("");
        let seg = after.split(['?', '#']).next().unwrap_or("");
        let mut parts = seg.split('/');
        let first = parts.next().unwrap_or("");
        return match first {
            "channel" | "c" | "user" => Ok(format!(
                "https://www.youtube.com/{first}/{}",
                parts.next().unwrap_or("")
            )),
            h if h.starts_with('@') => Ok(format!("https://www.youtube.com/{h}")),
            _ => Err("unsupported YouTube link".into()),
        };
    }
    if s.len() == 24 && s.starts_with("UC") {
        return Ok(format!("https://www.youtube.com/channel/{s}"));
    }
    let h = s.trim_start_matches('@').replace(' ', "");
    Ok(format!("https://www.youtube.com/@{h}"))
}

/// Resolve a handle / url / channel id into a [`Channel`].
pub fn resolve_channel(input: &str) -> Res<Channel> {
    let base = channel_url(input)?;
    let html = get_text(&format!("{base}/videos"))?;
    let data = initial_data(&html)?;
    let meta = data
        .pointer("/metadata/channelMetadataRenderer")
        .ok_or("channel not found")?;
    let id = meta["externalId"].as_str().ok_or("channel id missing")?.to_string();
    let name = meta["title"].as_str().unwrap_or("Unknown").to_string();
    let avatar = meta
        .pointer("/avatar/thumbnails/0/url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(Channel { id, name, avatar })
}

/// Collects every video found anywhere in a page's JSON.
pub(crate) fn collect(v: &Value, out: &mut Vec<Video>) {
    match v {
        Value::Object(m) => {
            if let Some(l) = m.get("shortsLockupViewModel") {
                if let Some(video) = parse_short(l) {
                    out.push(video);
                }
            } else if let Some(l) = m.get("lockupViewModel") {
                if l["contentType"] == "LOCKUP_CONTENT_TYPE_VIDEO" {
                    if let Some(video) = parse_lockup(l) {
                        out.push(video);
                    }
                }
            } else if let Some(r) = m
                .get("videoRenderer")
                .or_else(|| m.get("gridVideoRenderer"))
                .or_else(|| m.get("compactVideoRenderer"))
            {
                if let Some(video) = parse_renderer(r) {
                    out.push(video);
                }
            } else {
                for c in m.values() {
                    collect(c, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|c| collect(c, out)),
        _ => {}
    }
}

/// An empty Video with just an id and a title.
pub(crate) fn blank(id: &str, title: String) -> Video {
    Video {
        id: id.to_string(),
        title,
        thumb: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
        duration: String::new(),
        meta: String::new(),
        kind: Kind::Video,
        channel_id: String::new(),
        channel_name: String::new(),
        rank: 0,
        short: false,
        music: false,
    }
}

/// Reads a Short from a reel item.
pub(crate) fn parse_short(l: &Value) -> Option<Video> {
    let id = l
        .pointer("/onTap/innertubeCommand/reelWatchEndpoint/videoId")
        .and_then(Value::as_str)
        .or_else(|| l["entityId"].as_str().and_then(|e| e.strip_prefix("shorts-shelf-item-")))?;
    let a11y = l["accessibilityText"].as_str().unwrap_or("");
    // "<title>, 45 thousand views - play Short"
    let mut title = l
        .pointer("/overlayMetadata/primaryText/content")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| a11y.rsplit_once(", ").map_or(a11y, |x| x.0).to_string());
    if title.is_empty() {
        title = "Short".into();
    }
    let mut v = blank(id, title);
    v.short = true;
    v.thumb = l
        .pointer("/thumbnailViewModel/thumbnailViewModel/image/sources")
        .or_else(|| l.pointer("/thumbnail/sources"))
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .and_then(|s| s["url"].as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{id}/oar2.jpg"));
    v.meta = l
        .pointer("/overlayMetadata/secondaryText/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    v.duration = "SHORT".into();
    Some(v)
}

/// Reads a video from the newer "lockup" layout.
pub(crate) fn parse_lockup(l: &Value) -> Option<Video> {
    let id = l["contentId"].as_str()?;
    let md = &l["metadata"]["lockupMetadataViewModel"];
    let mut v = blank(id, md["title"]["content"].as_str()?.to_string());
    if let Some(src) = l
        .pointer("/contentImage/thumbnailViewModel/image/sources")
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .and_then(|s| s["url"].as_str())
    {
        v.thumb = src.to_string();
    }
    if let Some(overlays) = l
        .pointer("/contentImage/thumbnailViewModel/overlays")
        .and_then(Value::as_array)
    {
        for o in overlays {
            if let Some(badges) = o
                .pointer("/thumbnailBottomOverlayViewModel/badges")
                .and_then(Value::as_array)
            {
                for b in badges {
                    let b = &b["thumbnailBadgeViewModel"];
                    let text = b["text"].as_str().unwrap_or("");
                    let style = b["badgeStyle"].as_str().unwrap_or("");
                    if style.ends_with("LIVE") || text.eq_ignore_ascii_case("live") {
                        v.kind = Kind::Live;
                    } else if text.eq_ignore_ascii_case("upcoming") {
                        v.kind = Kind::Upcoming;
                    } else {
                        v.duration = text.to_string();
                    }
                }
            }
        }
    }
    let mut parts = vec![];
    if let Some(rows) = md
        .pointer("/metadata/contentMetadataViewModel/metadataRows")
        .and_then(Value::as_array)
    {
        for r in rows {
            for p in r["metadataParts"].as_array().into_iter().flatten() {
                let t = p["text"]["content"].as_str().unwrap_or("");
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
            }
        }
    }
    if v.kind == Kind::Video && parts.iter().any(|p| p.contains("watching")) {
        v.kind = Kind::Live;
    }
    v.meta = parts.join("  •  ");
    Some(v)
}

/// Reads a video from the classic renderer layout.
pub(crate) fn parse_renderer(r: &Value) -> Option<Video> {
    let id = r["videoId"].as_str()?;
    let mut v = blank(id, text_of(&r["title"]));
    if let Some(u) = r
        .pointer("/thumbnail/thumbnails")
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
    {
        v.thumb = u.to_string();
    }
    v.duration = text_of(&r["lengthText"]);
    for key in ["ownerText", "longBylineText", "shortBylineText"] {
        let name = text_of(&r[key]);
        if !name.is_empty() {
            v.channel_name = name;
            if let Some(id) = r
                .pointer(&format!("/{key}/runs/0/navigationEndpoint/browseEndpoint/browseId"))
                .and_then(Value::as_str)
            {
                v.channel_id = id.to_string();
            }
            break;
        }
    }
    let views = text_of(&r["viewCountText"]);
    let when = text_of(&r["publishedTimeText"]);
    v.meta = [views, when]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  •  ");
    let badges = r.to_string();
    if badges.contains("BADGE_STYLE_TYPE_LIVE_NOW") || badges.contains("\"style\":\"LIVE\"") {
        v.kind = Kind::Live;
    } else if r.get("upcomingEventData").is_some() {
        v.kind = Kind::Upcoming;
    }
    Some(v)
}

/// Recent uploads plus live / upcoming streams for a channel.
pub fn channel_videos(ch: &Channel) -> Res<Vec<Video>> {
    let base = format!("https://www.youtube.com/channel/{}", ch.id);
    let mut all: Vec<Video> = vec![];
    let mut first_err = None;
    let mut ok = 0;
    for tab in ["streams", "videos"] {
        match get_text(&format!("{base}/{tab}")).and_then(|h| initial_data(&h)) {
            Ok(data) => {
                ok += 1;
                let mut found = vec![];
                collect(&data, &mut found);
                for mut v in found {
                    if all.iter().any(|x| x.id == v.id) {
                        continue;
                    }
                    v.rank = all.len();
                    v.channel_id = ch.id.clone();
                    v.channel_name = ch.name.clone();
                    all.push(v);
                }
            }
            Err(e) => first_err = first_err.or(Some(e)),
        }
    }
    if ok == 0 {
        return Err(first_err.unwrap_or_else(|| "failed".into()));
    }
    // Live first, then upcoming, then uploads in channel order.
    let order = |k: &Kind| match k {
        Kind::Live => 0,
        Kind::Upcoming => 1,
        Kind::Video => 2,
    };
    all.sort_by_key(|v| (order(&v.kind), v.rank));
    Ok(all)
}

/// YouTube removed its Trending page, so "Live now" uses the search live filter instead.
pub fn live_now() -> Res<Vec<Video>> {
    let data = search_data("live", "EgJAAQ%3D%3D")?;
    let mut found = vec![];
    collect(&data, &mut found);
    let mut out: Vec<Video> = vec![];
    for mut v in found {
        if out.iter().any(|x| x.id == v.id) {
            continue;
        }
        v.kind = Kind::Live;
        v.rank = out.len();
        out.push(v);
    }
    Ok(out)
}

/// Reads a channel's name, handle, banner and counts.
pub(crate) fn parse_header(data: &Value) -> ChannelHeader {
    let mut h = ChannelHeader::default();
    let Some(pv) = data.pointer("/header/pageHeaderRenderer/content/pageHeaderViewModel") else {
        return h;
    };
    for row in pv
        .pointer("/metadata/contentMetadataViewModel/metadataRows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for part in row["metadataParts"].as_array().into_iter().flatten() {
            let t = part["text"]["content"].as_str().unwrap_or("");
            if t.starts_with('@') {
                h.handle = t.to_string();
            } else if t.contains("subscriber") {
                h.subs = t.to_string();
            } else if t.contains("video") {
                h.videos = t.to_string();
            }
        }
    }
    h.description = pv
        .pointer("/description/descriptionPreviewViewModel/description/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    h.banner = pv
        .pointer("/banner/imageBannerViewModel/image/sources")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().max_by_key(|s| s["width"].as_u64().unwrap_or(0)))
        .and_then(|s| s["url"].as_str())
        .unwrap_or("")
        .to_string();
    h
}

/// Reads a channel's About details.
pub(crate) fn parse_about(data: &Value) -> ChannelAbout {
    let mut a = ChannelAbout::default();
    let Some(v) = find_key(data, "aboutChannelViewModel") else { return a };
    let st = |k: &str| v[k].as_str().unwrap_or("").to_string();
    a.description = st("description").replace("\r\n", "\n").trim().to_string();
    a.subs = st("subscriberCountText");
    a.videos = st("videoCountText");
    a.views = st("viewCountText");
    a.joined = v
        .pointer("/joinedDateText/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches("Joined ")
        .to_string();
    a.country = st("country");
    for l in v["links"].as_array().into_iter().flatten() {
        let l = &l["channelExternalLinkViewModel"];
        let title = l.pointer("/title/content").and_then(Value::as_str).unwrap_or("");
        let link = l.pointer("/link/content").and_then(Value::as_str).unwrap_or("");
        if !link.is_empty() {
            let url = if link.contains("://") { link.to_string() } else { format!("https://{link}") };
            a.links.push((if title.is_empty() { link.to_string() } else { title.to_string() }, url));
        }
    }
    a
}

/// One tab of a channel page: "videos", "streams", "playlists" or "about".
/// The third value is the continuation token for the next page.
pub fn channel_tab(ch: &Channel, tab: &str) -> Res<(ChannelHeader, TabData, Option<String>)> {
    let html = get_text(&format!("https://www.youtube.com/channel/{}/{tab}", ch.id))?;
    let data = initial_data(&html)?;
    let header = parse_header(&data);
    // Only the selected tab's grid: the header carries unrelated "about" continuations.
    let next = if tab == "about" {
        None
    } else {
        data.pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
            .and_then(Value::as_array)
            .and_then(|tabs| {
                tabs.iter().find(|t| t.pointer("/tabRenderer/selected") == Some(&Value::Bool(true)))
            })
            .and_then(|t| t.pointer("/tabRenderer/content"))
            .and_then(last_continuation)
    };
    let td = match tab {
        "playlists" => {
            let mut out = vec![];
            collect_playlists(&data, &mut out);
            let mut seen = std::collections::HashSet::new();
            out.retain(|p| seen.insert(p.id.clone()));
            TabData::Playlists(out)
        }
        "about" => TabData::About(parse_about(&data)),
        _ => {
            let mut found = vec![];
            collect(&data, &mut found);
            let mut out: Vec<Video> = vec![];
            for mut v in found {
                if out.iter().any(|x| x.id == v.id) {
                    continue;
                }
                v.rank = out.len();
                v.channel_id = ch.id.clone();
                v.channel_name = ch.name.clone();
                out.push(v);
            }
            if tab == "streams" {
                let order = |k: &Kind| match k {
                    Kind::Live => 0,
                    Kind::Upcoming => 1,
                    Kind::Video => 2,
                };
                out.sort_by_key(|v| (order(&v.kind), v.rank));
            }
            TabData::Videos(out)
        }
    };
    Ok((header, td, next))
}

/// Search inside one channel: matching videos (including past streams) and playlists.
pub fn channel_search(ch: &Channel, query: &str) -> Res<(Vec<Video>, Vec<PlaylistHit>)> {
    let q = query.trim();
    if q.is_empty() {
        return Err("type something to search for".into());
    }
    let html = get_text(&format!(
        "https://www.youtube.com/channel/{}/search?query={}",
        ch.id,
        urlenc(q)
    ))?;
    let data = initial_data(&html)?;
    let mut found = vec![];
    collect(&data, &mut found);
    let mut videos: Vec<Video> = vec![];
    for mut v in found {
        if videos.iter().any(|x| x.id == v.id) {
            continue;
        }
        v.rank = videos.len();
        v.channel_id = ch.id.clone();
        v.channel_name = ch.name.clone();
        videos.push(v);
    }
    let mut pls = vec![];
    collect_playlists(&data, &mut pls);
    let mut seen = std::collections::HashSet::new();
    pls.retain(|p| seen.insert(p.id.clone()));
    Ok((videos, pls))
}

/// Streams that are live right now on a channel (cheap: only the Live tab is read).
pub fn channel_live(ch: &Channel) -> Res<Vec<Video>> {
    match channel_tab(ch, "streams")? {
        (_, TabData::Videos(v), _) => Ok(v.into_iter().filter(|x| x.kind == Kind::Live).collect()),
        _ => Ok(vec![]),
    }
}

/// Next page of a channel's videos / streams / playlists via the continuation token.
pub fn channel_more(ch: &Channel, tab: &str, token: &str) -> Res<(TabData, Option<String>)> {
    let resp = web_post("browse", json!({ "continuation": token }))?;
    let next = last_continuation(&resp);
    let td = if tab == "playlists" {
        let mut out = vec![];
        collect_playlists(&resp, &mut out);
        TabData::Playlists(out)
    } else {
        let mut found = vec![];
        collect(&resp, &mut found);
        for v in &mut found {
            v.channel_id = ch.id.clone();
            v.channel_name = ch.name.clone();
        }
        TabData::Videos(found)
    };
    Ok((td, next))
}

/// Collects the channels of a "Channels" grid (a channel's public subscriptions, or the channels
/// it features) from a page or a continuation response.
pub(crate) fn collect_grid_channels(v: &Value, out: &mut Vec<ChannelHit>) {
    match v {
        Value::Object(m) => {
            if let Some(r) = m.get("gridChannelRenderer").or_else(|| m.get("channelRenderer")) {
                if let Some(id) = r["channelId"].as_str() {
                    let avatar = r
                        .pointer("/thumbnail/thumbnails")
                        .and_then(Value::as_array)
                        .and_then(|a| a.last())
                        .and_then(|t| t["url"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let avatar = if avatar.starts_with("//") { format!("https:{avatar}") } else { avatar };
                    let handle = r
                        .pointer("/navigationEndpoint/browseEndpoint/canonicalBaseUrl")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim_start_matches('/')
                        .to_string();
                    out.push(ChannelHit {
                        channel: Channel { id: id.to_string(), name: text_of(&r["title"]), avatar },
                        handle: if handle.starts_with('@') { handle } else { String::new() },
                        subs: text_of(&r["subscriberCountText"]),
                    });
                }
            } else {
                m.values().for_each(|c| collect_grid_channels(c, out));
            }
        }
        Value::Array(a) => a.iter().for_each(|c| collect_grid_channels(c, out)),
        _ => {}
    }
}

/// The channels a channel page shows publicly: its subscriptions when it made them public (the
/// "Channels" tab), otherwise the channels it features on its home page. Without signing in
/// nothing else can be read. The channel itself is never listed.
pub fn public_channels(ch: &Channel) -> Res<Vec<ChannelHit>> {
    for tab in ["channels", "featured"] {
        let Ok(html) = get_text(&format!("https://www.youtube.com/channel/{}/{tab}", ch.id)) else { continue };
        let Ok(data) = initial_data(&html) else { continue };
        let mut found = vec![];
        collect_grid_channels(&data, &mut found);
        if found.is_empty() {
            continue;
        }
        // long lists continue on further pages
        let mut token = data
            .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
            .and_then(Value::as_array)
            .and_then(|tabs| tabs.iter().find(|t| t.pointer("/tabRenderer/selected") == Some(&Value::Bool(true))))
            .and_then(|t| t.pointer("/tabRenderer/content"))
            .and_then(last_continuation);
        let mut pages = 0;
        while let (Some(t), true) = (token.take(), pages < 12) {
            pages += 1;
            let Ok(resp) = web_post("browse", json!({ "continuation": t })) else { break };
            let before = found.len();
            collect_grid_channels(&resp, &mut found);
            if found.len() == before {
                break;
            }
            token = last_continuation(&resp);
        }
        let mut seen = std::collections::HashSet::new();
        found.retain(|h| h.channel.id != ch.id && seen.insert(h.channel.id.clone()));
        return Ok(found);
    }
    Ok(vec![])
}
