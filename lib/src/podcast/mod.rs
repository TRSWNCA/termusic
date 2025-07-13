// Thanks to the author of shellcaster(https://github.com/jeff-hughes/shellcaster). Most parts of following code are taken from it.

pub mod db;
#[allow(clippy::module_name_repetitions)]
pub mod episode;
// repetetive name, but will do for now
#[allow(clippy::module_inception)]
mod podcast;

use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use bytes::Buf;
use chrono::{DateTime, Utc};
use opml::{Body, Head, OPML, Outline};
use regex::Regex;
use reqwest::ClientBuilder;
use rfc822_sanitizer::parse_from_rfc2822_with_fallback;
use rss::{Channel, Item};
use sanitize_filename::{Options, sanitize_with_options};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::config::v2::server::PodcastSettings;
use crate::taskpool::TaskPool;
use crate::types::{Msg, PCMsg};
use db::Database;
use episode::{Episode, EpisodeNoId};
pub use podcast::{Podcast, PodcastNoId};

// How many columns we need, minimum, before we display the
// (unplayed/total) after the podcast title
pub const PODCAST_UNPLAYED_TOTALS_LENGTH: usize = 25;

// How many columns we need, minimum, before we display the duration of
// the episode
pub const EPISODE_DURATION_LENGTH: usize = 45;

// How many columns we need, minimum, before we display the pubdate
// of the episode
pub const EPISODE_PUBDATE_LENGTH: usize = 60;

/// Regex for parsing an episode "duration", which could take the form
/// of HH:MM:SS, MM:SS, or SS.
static RE_DURATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)(?::(\d+))?(?::(\d+))?").unwrap());

/// Regex for removing "A", "An", and "The" from the beginning of
/// podcast titles
static RE_ARTICLES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(a|an|the) ").unwrap());

/// Defines interface used for both podcasts and episodes, to be
/// used and displayed in menus.
// TODO: unused trait & functions?
pub trait Menuable {
    fn get_id(&self) -> i64;
    fn get_title(&self, length: usize) -> String;
    fn is_played(&self) -> bool;
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct PodcastFeed {
    pub id: Option<i64>,
    pub url: String,
    pub title: Option<String>,
}

impl PodcastFeed {
    #[must_use]
    pub const fn new(id: Option<i64>, url: String, title: Option<String>) -> Self {
        Self { id, url, title }
    }
}

/// Spawns a new task to check a feed and retrieve podcast data.
///
/// If `tx_to_main` is closed, no errors will be throws and the task will continue
pub fn check_feed(
    feed: PodcastFeed,
    max_retries: usize,
    tp: &TaskPool,
    tx_to_main: UnboundedSender<Msg>,
) {
    tp.execute(async move {
        let _ = tx_to_main.send(Msg::Podcast(PCMsg::FetchPodcastStart(feed.url.clone())));
        match get_feed_data(&feed.url, max_retries).await {
            Ok(pod) => match feed.id {
                Some(id) => {
                    let _ = tx_to_main.send(Msg::Podcast(PCMsg::SyncData((id, pod))));
                }
                None => {
                    let _ = tx_to_main.send(Msg::Podcast(PCMsg::NewData(pod)));
                }
            },
            Err(err) => {
                error!("get_feed_data had a Error: {err:#?}");
                let _ = tx_to_main.send(Msg::Podcast(PCMsg::Error(feed)));
            }
        }
    });
}

/// Given a URL, this attempts to pull the data about a podcast and its
/// episodes from an RSS feed.
async fn get_feed_data(url: &str, mut max_retries: usize) -> Result<PodcastNoId> {
    let agent = ClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .build()?;

    let resp: reqwest::Response = loop {
        let response = agent.get(url).send().await;
        if let Ok(resp) = response {
            break resp;
        }
        max_retries -= 1;
        if max_retries == 0 {
            bail!("No response from feed");
        }
    };

    let channel = Channel::read_from(resp.bytes().await?.reader())?;
    Ok(parse_feed_data(channel, url))
}

/// Given a Channel with the RSS feed data, this parses the data about a
/// podcast and its episodes and returns a Podcast. There are existing
/// specifications for podcast RSS feeds that a feed should adhere to, but
/// this does try to make some attempt to account for the possibility that
/// a feed might not be valid according to the spec.
fn parse_feed_data(channel: Channel, url: &str) -> PodcastNoId {
    let title = channel.title().to_string();
    let url = url.to_string();
    let description = Some(channel.description().to_string());
    let last_checked = Utc::now();

    let mut author = None;
    let mut explicit = None;
    let mut image_url = None;
    if let Some(itunes) = channel.itunes_ext() {
        author = itunes.author().map(std::string::ToString::to_string);
        explicit = itunes.explicit().and_then(|s| {
            let ss = s.to_lowercase();
            match &ss[..] {
                "yes" | "explicit" | "true" => Some(true),
                "no" | "clean" | "false" => Some(false),
                _ => None,
            }
        });
        image_url = itunes.image().map(std::string::ToString::to_string);
    }

    let mut episodes = Vec::new();
    let items = channel.into_items();
    if !items.is_empty() {
        for item in &items {
            episodes.push(parse_episode_data(item));
        }
    }

    PodcastNoId {
        title,
        url,
        description,
        author,
        explicit,
        last_checked,
        episodes,
        image_url,
    }
}

/// For an item (episode) in an RSS feed, this pulls data about the item
/// and converts it to an Episode. There are existing specifications for
/// podcast RSS feeds that a feed should adhere to, but this does try to
/// make some attempt to account for the possibility that a feed might
/// not be valid according to the spec.
fn parse_episode_data(item: &Item) -> EpisodeNoId {
    let title = item.title().unwrap_or("").to_string();
    let url = match item.enclosure() {
        Some(enc) => enc.url().to_string(),
        None => String::new(),
    };
    let guid = match item.guid() {
        Some(guid) => guid.value().to_string(),
        None => String::new(),
    };
    let description = item.description().unwrap_or("").to_string();
    let pubdate = item
        .pub_date()
        .and_then(|pd| parse_from_rfc2822_with_fallback(pd).ok())
        .map(std::convert::Into::into);

    let mut duration = None;
    let mut image_url = None;
    if let Some(itunes) = item.itunes_ext() {
        duration = duration_to_int(itunes.duration()).map(i64::from);
        image_url = itunes.image().map(std::string::ToString::to_string);
    }

    EpisodeNoId {
        title,
        url,
        guid,
        description,
        pubdate,
        duration,
        image_url,
    }
}

/// Given a string representing an episode duration, this attempts to
/// convert to an integer representing the duration in seconds. Covers
/// formats HH:MM:SS, MM:SS, and SS. If the duration cannot be converted
/// (covering numerous reasons), it will return None.
fn duration_to_int(duration: Option<&str>) -> Option<i32> {
    let duration = duration?;
    let captures = RE_DURATION.captures(duration)?;

    /*
     * Provided that the regex succeeds, we should have
     * 4 capture groups (with 0th being the full match).
     * Depending on the string format, however, some of
     * these may return None. We first loop through the
     * capture groups and push Some results to an array.
     * This will fail on the first non-numeric value,
     * so the duration is parsed only if all components
     * of it were successfully converted to integers.
     * Finally, we convert hours, minutes, and seconds
     * into a total duration in seconds and return.
     */

    let mut times = [None; 3];
    let mut counter = 0;
    // cap[0] is always full match
    for c in captures.iter().skip(1).flatten() {
        let intval = c.as_str().parse().ok()?;
        times[counter] = Some(intval);
        counter += 1;
    }

    match counter {
        // HH:MM:SS
        3 => Some(times[0].unwrap() * 60 * 60 + times[1].unwrap() * 60 + times[2].unwrap()),
        // MM:SS
        2 => Some(times[0].unwrap() * 60 + times[1].unwrap()),
        // SS
        1 => times[0],
        _ => None,
    }
}

/// Imports a list of podcasts from OPML format, reading from a file. If the `replace` flag is set, this replaces all
/// existing data in the database.
pub async fn import_from_opml(db_path: &Path, config: &PodcastSettings, file: &Path) -> Result<()> {
    let xml = std::fs::read_to_string(file)
        .with_context(|| format!("Could not open OPML file: {}", file.display()))?;

    let mut podcast_list = import_opml_feeds(&xml).with_context(
        || "Could not properly parse OPML file -- file may be formatted improperly or corrupted.",
    )?;

    if podcast_list.is_empty() {
        println!("No podcasts to import.");
        return Ok(());
    }

    let db_inst = db::Database::new(db_path)?;

    // delete database if we are replacing the data
    // if args.is_present("replace") {
    //     db_inst
    //         .clear_db()
    //         .with_context(|| "Error clearing database")?;
    // } else {
    let old_podcasts = db_inst.get_podcasts()?;

    // if URL is already in database, remove it from import
    podcast_list.retain(|pod| {
        for op in &old_podcasts {
            if pod.url == op.url {
                return false;
            }
        }
        true
    });
    // }

    // check again, now that we may have removed feeds after looking at
    // the database
    if podcast_list.is_empty() {
        println!("No podcasts to import.");
        return Ok(());
    }

    println!("Importing {} podcasts...", podcast_list.len());

    let taskpool = TaskPool::new(usize::from(config.concurrent_downloads_max.get()));
    let (tx_to_main, mut rx_to_main) = unbounded_channel();

    for pod in &podcast_list {
        check_feed(
            pod.clone(),
            usize::from(config.max_download_retries),
            &taskpool,
            tx_to_main.clone(),
        );
    }

    let mut msg_counter: usize = 0;
    let mut failure = false;
    while let Some(message) = rx_to_main.recv().await {
        match message {
            Msg::Podcast(PCMsg::NewData(pod)) => {
                msg_counter += 1;
                let title = &pod.title;
                let db_result = db_inst.insert_podcast(&pod);
                match db_result {
                    Ok(_) => {
                        println!("Added {title}");
                    }
                    Err(err) => {
                        failure = true;
                        error!("Error adding {title}, err: {err}");
                    }
                }
            }

            Msg::Podcast(PCMsg::Error(feed)) => {
                msg_counter += 1;
                failure = true;
                error!("Error retrieving RSS feed: {}", feed.url);
            }

            Msg::Podcast(PCMsg::SyncData((_id, _pod))) => {
                msg_counter += 1;
            }
            _ => {}
        }

        if msg_counter >= podcast_list.len() {
            break;
        }
    }

    if failure {
        bail!("Process finished with errors.");
    }
    println!("Import successful.");

    Ok(())
}

/// Exports all podcasts to OPML format, either printing to stdout or
/// exporting to a file.
pub fn export_to_opml(db_path: &Path, file: &Path) -> Result<()> {
    let db_inst = Database::new(db_path)?;
    let podcast_list = db_inst.get_podcasts()?;
    let opml = export_opml_feeds(&podcast_list);

    let xml = opml.to_string().context("Could not create OPML format")?;

    let mut dst = File::create(file)
        .with_context(|| format!("Could not create output file: {}", file.display()))?;
    dst.write_all(xml.as_bytes()).with_context(|| {
        format!(
            "Could not copy OPML data to output file: {}",
            file.display()
        )
    })?;
    Ok(())
}

/// Import a list of podcast feeds from an OPML file. Supports
/// v1.0, v1.1, and v2.0 OPML files.
fn import_opml_feeds(xml: &str) -> Result<Vec<PodcastFeed>> {
    let opml = OPML::from_str(xml)?;
    let mut feeds = Vec::new();
    for pod in opml.body.outlines {
        if pod.xml_url.is_some() {
            // match against title attribute first -- if this is
            // not set or empty, then match against the text
            // attribute; this must be set, but can be empty
            let title = pod.title.filter(|t| !t.is_empty()).or({
                if pod.text.is_empty() {
                    None
                } else {
                    Some(pod.text)
                }
            });
            feeds.push(PodcastFeed::new(None, pod.xml_url.unwrap(), title));
        }
    }
    Ok(feeds)
}

/// Converts the current set of podcast feeds to the OPML format
fn export_opml_feeds(podcasts: &[Podcast]) -> OPML {
    let date = Utc::now();
    let mut opml = OPML {
        head: Some(Head {
            title: Some("Termusic Podcast Feeds".to_string()),
            date_created: Some(date.to_rfc2822()),
            ..Head::default()
        }),
        ..Default::default()
    };

    let mut outlines = Vec::new();

    for pod in podcasts {
        // opml.add_feed(&pod.title, &pod.url);
        outlines.push(Outline {
            text: pod.title.clone(),
            r#type: Some("rss".to_string()),
            xml_url: Some(pod.url.clone()),
            title: Some(pod.title.clone()),
            ..Outline::default()
        });
    }

    opml.body = Body { outlines };
    opml
}

/// Enum used to communicate relevant data to the taskpool.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EpData {
    pub id: i64,
    pub pod_id: i64,
    pub title: String,
    pub url: String,
    pub pubdate: Option<DateTime<Utc>>,
    pub file_path: Option<PathBuf>,
}

/// This is the function the main controller uses to indicate new files to download.
///
/// It uses the taskpool to start jobs for every episode to be downloaded.
/// New jobs can be requested by the user while there are still ongoing jobs.
///
/// If `tx_to_main` is closed, no errors will be throws and the task will continue
pub fn download_list(
    episodes: Vec<EpData>,
    dest: &Path,
    max_retries: usize,
    tp: &TaskPool,
    tx_to_main: &UnboundedSender<Msg>,
) {
    // parse episode details and push to queue
    for ep in episodes {
        let tx = tx_to_main.clone();
        let dest2 = dest.to_path_buf();
        tp.execute(async move {
            let _ = tx.send(Msg::Podcast(PCMsg::DLStart(ep.clone())));
            let result = download_file(ep, dest2, max_retries).await;
            let _ = tx.send(Msg::Podcast(result));
        });
    }
}

/// Downloads a file to a local filepath, returning `DownloadMsg` variant
/// indicating success or failure.
async fn download_file(
    mut ep_data: EpData,
    destination_path: PathBuf,
    mut max_retries: usize,
) -> PCMsg {
    let agent = ClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("reqwest client build failed");

    let response: reqwest::Response = loop {
        let response = agent.get(&ep_data.url).send().await;
        if let Ok(resp) = response {
            break resp;
        }
        max_retries -= 1;
        if max_retries == 0 {
            return PCMsg::DLResponseError(ep_data);
        }
    };

    // figure out the file type
    let ext = if let Some(content_type) = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
    {
        match content_type {
            "audio/x-m4a" | "audio/mp4" => "m4a",
            "audio/x-matroska" => "mka",
            "audio/flac" => "flac",
            "video/quicktime" => "mov",
            "video/mp4" => "mp4",
            "video/x-m4v" => "m4v",
            "video/x-matroska" => "mkv",
            "video/webm" => "webm",
            // "audio/mpeg" => "mp3",
            // fallback
            _ => "mp3",
        }
    } else {
        error!("The response doesn't contain a content type, using \"mp3\" as fallback!");
        "mp3"
    };

    let mut file_name = sanitize_with_options(
        &ep_data.title,
        Options {
            truncate: true,
            windows: true, // for simplicity, we'll just use Windows-friendly paths for everyone
            replacement: "",
        },
    );

    if let Some(pubdate) = ep_data.pubdate {
        file_name = format!("{file_name}_{}", pubdate.format("%Y%m%d_%H%M%S"));
    }

    let mut file_path = destination_path;
    file_path.push(format!("{file_name}.{ext}"));

    let Ok(mut dst) = File::create(&file_path) else {
        return PCMsg::DLFileCreateError(ep_data);
    };

    ep_data.file_path = Some(file_path);

    let Ok(bytes) = response.bytes().await else {
        return PCMsg::DLFileCreateError(ep_data);
    };

    match std::io::copy(&mut bytes.reader(), &mut dst) {
        Ok(_) => PCMsg::DLComplete(ep_data),
        Err(_) => PCMsg::DLFileWriteError(ep_data),
    }
}
