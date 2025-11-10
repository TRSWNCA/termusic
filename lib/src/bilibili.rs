use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use bpi_rs::{
    BpiClient,
    search::{
        result::Video as ApiVideo,
        search_params::{Duration as ApiDuration, SearchOrder},
    },
};
use regex::Regex;

/// Information about a single video returned by a Bilibili search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BilibiliVideo {
    pub title: String,
    pub duration_seconds: u64,
    pub bvid: String,
    pub author: String,
    pub url: String,
}

/// Paginated search results for Bilibili.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BilibiliPage {
    pub items: Vec<BilibiliVideo>,
    pub page: u32,
    pub total_pages: u32,
}

/// A reusable search session that keeps track of the keyword and allows fetching additional pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchSession {
    keyword: String,
}

impl SearchSession {
    /// Create a new search session and return the first page of results.
    ///
    /// # Errors
    ///
    /// Returns an error if the query is empty or the upstream request fails.
    pub async fn new(keyword: &str) -> Result<(Self, BilibiliPage)> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            bail!("Search keyword cannot be empty");
        }

        let session = Self {
            keyword: keyword.to_string(),
        };

        let page = session.fetch_page(1).await?;
        Ok((session, page))
    }

    /// Fetch an arbitrary results page for the current session.
    ///
    /// # Errors
    ///
    /// Returns an error if the page number cannot be converted or the upstream request fails.
    pub async fn fetch_page(&self, page: u32) -> Result<BilibiliPage> {
        let page_i32 = i32::try_from(page).context("page number exceeds i32 range")?;
        fetch_page(&self.keyword, page_i32).await
    }
}

static TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("valid regex"));

async fn fetch_page(keyword: &str, page: i32) -> Result<BilibiliPage> {
    let client = BpiClient::new();
    let response = client
        .search_video(
            keyword,
            Some(SearchOrder::TotalRank),
            Some(ApiDuration::All),
            None,
            Some(page),
        )
        .await?;

    let data = response
        .data
        .ok_or_else(|| anyhow!("No search data returned from Bilibili"))?;

    let results = data.result.unwrap_or_default();
    let videos = results.into_iter().map(convert_video).collect::<Vec<_>>();

    let page = u32::try_from(data.page).unwrap_or(1).max(1);
    let total_pages = u32::try_from(data.num_pages).unwrap_or(page).max(1);

    Ok(BilibiliPage {
        items: videos,
        page,
        total_pages,
    })
}

fn convert_video(video: ApiVideo) -> BilibiliVideo {
    let title = clean_text(&video.title);
    let author = clean_text(&video.author);
    let duration_seconds = parse_duration(&video.duration).unwrap_or_default();
    let url = format!("https://www.bilibili.com/video/{}", video.bvid);

    BilibiliVideo {
        title,
        duration_seconds,
        bvid: video.bvid,
        author,
        url,
    }
}

fn clean_text(input: &str) -> String {
    let without_tags = TAG_REGEX.replace_all(input, "");
    decode_basic_entities(without_tags.trim())
}

fn parse_duration(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut total = 0u64;
    let parts: Vec<_> = trimmed.split(':').collect();
    let len = parts.len();

    for (idx, part) in parts.into_iter().enumerate() {
        let value: u64 = part.parse().ok()?;
        let power = (len - 1 - idx) as u32;
        total = total.saturating_add(value * 60u64.pow(power));
    }

    Some(total)
}

fn decode_basic_entities(input: &str) -> String {
    let mut output = input.replace("&amp;", "&");
    output = output.replace("&lt;", "<");
    output = output.replace("&gt;", ">");
    output = output.replace("&quot;", "\"");
    output = output.replace("&#39;", "'");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_handles_mm_ss() {
        assert_eq!(parse_duration("03:15"), Some(195));
    }

    #[test]
    fn parse_duration_handles_hh_mm_ss() {
        assert_eq!(parse_duration("01:02:03"), Some(3723));
    }

    #[test]
    fn parse_duration_handles_invalid() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
    }

    #[test]
    fn clean_text_removes_tags_and_decodes_entities() {
        let input = "<em class=\"keyword\">Rust</em> &amp; 冒险";
        assert_eq!(clean_text(input), "Rust & 冒险");
    }
}
