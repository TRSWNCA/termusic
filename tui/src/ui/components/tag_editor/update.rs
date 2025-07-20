use crate::ui::Model;
use termusiclib::ids::{Id, IdTagEditor};
use termusiclib::types::{TEMsg, TFMsg};

impl Model {
    pub fn update_tageditor(&mut self, msg: TEMsg) {
        match msg {
            TEMsg::TagEditorRun(node_id) => {
                self.mount_tageditor(&node_id);
            }
            TEMsg::TagEditorClose => {
                if let Some(s) = self.tageditor_song.clone() {
                    // TODO: this should be re-done and take actual track ids themself, or at least verified to use the same functions to result in the same id
                    self.library_reload_with_node_focus(Some(s.path_as_id_str().to_string()));
                }
                self.umount_tageditor();
            }

            TEMsg::TECounterDeleteOk => {
                self.te_delete_lyric();
            }
            TEMsg::TESelectLyricOk(index) => {
                if let Some(mut song) = self.tageditor_song.take() {
                    song.set_lyric_selected_index(index);
                    // the unwrap should also never happen as all components should be properly mounted
                    self.init_by_song(song).unwrap();
                }
            }
            TEMsg::TESearch => {
                self.te_songtag_search();
            }
            TEMsg::TEDownload(index) => {
                if let Err(e) = self.te_songtag_download(index) {
                    self.mount_error_popup(e.context("download by songtag"));
                }
            }
            TEMsg::TEEmbed(index) => {
                if let Err(e) = self.te_load_lyric_and_photo(index) {
                    self.mount_error_popup(e.context("log lyric and photo"));
                }
            }
            TEMsg::TERename => {
                if let Err(e) = self.te_rename_song_by_tag() {
                    self.mount_error_popup(e.context("rename song by tag"));
                }
            }
            TEMsg::TEFocus(m) => self.update_tag_editor_focus(m),

            TEMsg::TESearchLyricResult(m) => self.te_update_lyric_results(m),
        }
    }

    fn update_tag_editor_focus(&mut self, msg: TFMsg) {
        match msg {
            TFMsg::TextareaLyricBlurDown | TFMsg::InputTitleBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::InputArtist))
                    .ok();
            }
            TFMsg::InputArtistBlurDown | TFMsg::InputAlbumBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::InputTitle))
                    .ok();
            }
            TFMsg::InputTitleBlurDown | TFMsg::InputGenreBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::InputAlbum))
                    .ok();
            }
            TFMsg::InputAlbumBlurDown | TFMsg::TableLyricOptionsBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::InputGenre))
                    .ok();
            }
            TFMsg::InputGenreBlurDown | TFMsg::SelectLyricBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::TableLyricOptions))
                    .ok();
            }
            TFMsg::TableLyricOptionsBlurDown | TFMsg::CounterDeleteBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::SelectLyric))
                    .ok();
            }
            TFMsg::SelectLyricBlurDown | TFMsg::TextareaLyricBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::CounterDelete))
                    .ok();
            }
            TFMsg::CounterDeleteBlurDown | TFMsg::InputArtistBlurUp => {
                self.app
                    .active(&Id::TagEditor(IdTagEditor::TextareaLyric))
                    .ok();
            }
        }
    }
}
