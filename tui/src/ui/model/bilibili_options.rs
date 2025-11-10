use std::future::Future;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use termusiclib::bilibili::{BilibiliPage, BilibiliVideo, SearchSession};
use termusiclib::track::DurationFmtShort;
use tuirealm::props::{Alignment, AttrValue, Attribute, TableBuilder, TextSpan};

use super::{Model, youtube_options::DownloadContext};
use crate::ui::ids::Id;
use crate::ui::msg::{BSMsg, Msg};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BilibiliData {
    pub items: Vec<BilibiliVideo>,
    pub page: u32,
    pub total_pages: u32,
}

impl From<BilibiliPage> for BilibiliData {
    fn from(value: BilibiliPage) -> Self {
        Self {
            items: value.items,
            page: value.page,
            total_pages: value.total_pages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BilibiliOptions {
    pub data: BilibiliData,
    pub session: Option<SearchSession>,
}

impl BilibiliOptions {
    pub fn get_by_index(&self, index: usize) -> Result<&BilibiliVideo> {
        self.data
            .items
            .get(index)
            .ok_or_else(|| anyhow!("index not found"))
    }

    pub fn is_empty(&self) -> bool {
        self.data.items.is_empty()
    }

    pub fn items(&self) -> &[BilibiliVideo] {
        &self.data.items
    }

    pub fn page(&self) -> u32 {
        self.data.page
    }

    pub fn total_pages(&self) -> u32 {
        self.data.total_pages
    }

    pub fn get_prev_page(
        &self,
    ) -> Option<impl Future<Output = Result<BilibiliData>> + Send + 'static> {
        if self.data.page <= 1 {
            return None;
        }

        let target_page = self.data.page - 1;
        let session = self.session.clone()?;
        Some(async move {
            let page = session.fetch_page(target_page).await?;
            Ok(BilibiliData::from(page))
        })
    }

    pub fn get_next_page(
        &self,
    ) -> Option<impl Future<Output = Result<BilibiliData>> + Send + 'static> {
        if self.data.page >= self.data.total_pages {
            return None;
        }

        let target_page = self.data.page + 1;
        let session = self.session.clone()?;
        Some(async move {
            let page = session.fetch_page(target_page).await?;
            Ok(BilibiliData::from(page))
        })
    }
}

impl Model {
    pub fn bilibili_options_download(&mut self, index: usize) -> Result<()> {
        if let Ok(item) = self.bilibili_options.get_by_index(index) {
            if let Err(e) =
                self.download_with_ytdlp(&item.url, "bilibili", DownloadContext::Bilibili)
            {
                bail!("Download error: {e}");
            }
        }
        Ok(())
    }

    pub fn bilibili_options_search(&mut self, keyword: String) {
        let tx = self.tx_to_main.clone();
        tokio::spawn(async move {
            match SearchSession::new(&keyword).await {
                Ok((session, page)) => {
                    let options = BilibiliOptions {
                        data: BilibiliData::from(page),
                        session: Some(session),
                    };
                    tx.send(Msg::BilibiliSearch(BSMsg::SearchSuccess(options)))
                        .ok();
                }
                Err(e) => {
                    tx.send(Msg::BilibiliSearch(BSMsg::SearchFail(e.to_string())))
                        .ok();
                }
            }
        });
    }

    pub fn bilibili_options_prev_page(&self) {
        let tx_to_main = self.tx_to_main.clone();

        let Some(fut) = self.bilibili_options.get_prev_page() else {
            return;
        };

        tokio::task::spawn(async move {
            match fut.await {
                Ok(data) => {
                    let _ = tx_to_main.send(Msg::BilibiliSearch(BSMsg::PageLoaded(data)));
                }
                Err(err) => {
                    let _ = tx_to_main.send(Msg::BilibiliSearch(BSMsg::PageLoadError(
                        err.to_string(),
                    )));
                }
            }
        });
    }

    pub fn bilibili_options_next_page(&mut self) {
        let tx_to_main = self.tx_to_main.clone();

        let Some(fut) = self.bilibili_options.get_next_page() else {
            return;
        };

        tokio::task::spawn(async move {
            match fut.await {
                Ok(data) => {
                    let _ = tx_to_main.send(Msg::BilibiliSearch(BSMsg::PageLoaded(data)));
                }
                Err(err) => {
                    let _ = tx_to_main.send(Msg::BilibiliSearch(BSMsg::PageLoadError(
                        err.to_string(),
                    )));
                }
            }
        });
    }

    pub fn sync_bilibili_options(&mut self) {
        if self.bilibili_options.is_empty() {
            let table = TableBuilder::default()
                .add_col(TextSpan::from("No results."))
                .add_col(TextSpan::from(
                    "Nothing was found in 10 seconds, connection issue encountered.",
                ))
                .build();
            self.app
                .attr(
                    &Id::BilibiliSearchTablePopup,
                    Attribute::Content,
                    AttrValue::Table(table),
                )
                .ok();
            return;
        }

        let mut table: TableBuilder = TableBuilder::default();
        for (idx, record) in self.bilibili_options.items().iter().enumerate() {
            if idx > 0 {
                table.add_row();
            }
            let duration = DurationFmtShort(Duration::from_secs(record.duration_seconds));
            let duration_string = format!("[{duration:^10.10}]");
            let title = record.title.as_str();

            table
                .add_col(TextSpan::new(duration_string))
                .add_col(TextSpan::new(title).bold());
        }
        let table = table.build();
        self.app
            .attr(
                &Id::BilibiliSearchTablePopup,
                Attribute::Content,
                AttrValue::Table(table),
            )
            .ok();

        let title = format!(
            "\u{2500}\u{2500}\u{2500} Page {} / {} \u{2500}\u{2500}\u{2500}\u{2524} {} \u{251c}\u{2500}\u{2500} bilibili.com \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            self.bilibili_options.page(),
            self.bilibili_options.total_pages(),
            "Tab/Shift+Tab switch pages",
        );
        self.app
            .attr(
                &Id::BilibiliSearchTablePopup,
                Attribute::Title,
                AttrValue::Title((title, Alignment::Left)),
            )
            .ok();
    }
}
