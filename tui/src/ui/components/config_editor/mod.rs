/**
 * MIT License
 *
 * tuifeed - Copyright (c) 2021 Christian Visintin
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use anyhow::Result;
use termusiclib::config::{SharedTuiSettings, TuiOverlay};
use termusiclib::ids::{Id, IdConfigEditor};
use tui_realm_stdlib::{Radio, Span};
use tuirealm::props::{Alignment, BorderSides, BorderType, Borders, Style, TextSpan};
use tuirealm::{Component, Event, MockComponent};

use super::popups::{YNConfirm, YNConfirmStyle};
use crate::ui::model::{ConfigEditorLayout, Model, UserEvent};
use crate::ui::msg::{ConfigEditorMsg, Msg};

mod color;
mod general;
mod key_combo;
mod update;
mod view;

#[derive(MockComponent)]
pub struct CEHeader {
    component: Radio,
}

impl CEHeader {
    pub fn new(layout: ConfigEditorLayout, config: &TuiOverlay) -> Self {
        Self {
            component: Radio::default()
                .borders(
                    Borders::default()
                        .modifiers(BorderType::Plain)
                        .sides(BorderSides::BOTTOM),
                )
                .choices([
                    "General Configuration",
                    "Themes and Colors",
                    "Keys Global",
                    "Keys Other",
                ])
                .foreground(config.settings.theme.library_highlight())
                .inactive(Style::default().fg(config.settings.theme.library_highlight()))
                .value(match layout {
                    ConfigEditorLayout::General => 0,
                    ConfigEditorLayout::Color => 1,
                    ConfigEditorLayout::Key1 => 2,
                    ConfigEditorLayout::Key2 => 3,
                }),
        }
    }
}

impl Component<Msg, UserEvent> for CEHeader {
    fn on(&mut self, _ev: Event<UserEvent>) -> Option<Msg> {
        None
    }
}

#[derive(MockComponent)]
pub struct CEFooter {
    component: Span,
}

impl CEFooter {
    pub fn new(config: &TuiOverlay) -> Self {
        Self {
            component: Span::default().spans([
                TextSpan::new(" Save parameters: ").bold(),
                TextSpan::new(format!("<{}>", config.settings.keys.config_keys.save))
                    .bold()
                    .fg(config.settings.theme.library_highlight()),
                TextSpan::new(" Exit: ").bold(),
                TextSpan::new(format!("<{}>", config.settings.keys.escape))
                    .bold()
                    .fg(config.settings.theme.library_highlight()),
                TextSpan::new(" Change panel: ").bold(),
                TextSpan::new("<TAB>")
                    .bold()
                    .fg(config.settings.theme.library_highlight()),
                TextSpan::new(" Change field: ").bold(),
                TextSpan::new("<UP/DOWN>")
                    .bold()
                    .fg(config.settings.theme.library_highlight()),
                TextSpan::new(" Select theme/Preview symbol: ").bold(),
                TextSpan::new("<ENTER>")
                    .bold()
                    .fg(config.settings.theme.library_highlight()),
            ]),
        }
    }
}

impl Component<Msg, UserEvent> for CEFooter {
    fn on(&mut self, _ev: Event<UserEvent>) -> Option<Msg> {
        None
    }
}

#[derive(MockComponent)]
pub struct ConfigSavePopup {
    component: YNConfirm,
}

impl ConfigSavePopup {
    pub fn new(config: SharedTuiSettings) -> Self {
        let component =
            YNConfirm::new_with_cb(config, " Config changed. Do you want to save? ", |config| {
                YNConfirmStyle {
                    foreground_color: config.settings.theme.important_popup_foreground(),
                    background_color: config.settings.theme.important_popup_background(),
                    border_color: config.settings.theme.important_popup_border(),
                    title_alignment: Alignment::Center,
                }
            });
        Self { component }
    }
}

impl Component<Msg, UserEvent> for ConfigSavePopup {
    fn on(&mut self, ev: Event<UserEvent>) -> Option<Msg> {
        self.component.on(
            ev,
            Msg::ConfigEditor(ConfigEditorMsg::ConfigSaveOk),
            Msg::ConfigEditor(ConfigEditorMsg::ConfigSaveCancel),
        )
    }
}

impl Model {
    /// Mount / Remount the Config-Editor's Header & Footer
    fn remount_config_header_footer(&mut self) -> Result<()> {
        self.app.remount(
            Id::ConfigEditor(IdConfigEditor::Header),
            Box::new(CEHeader::new(
                self.config_editor.layout,
                &self.config_tui.read(),
            )),
            Vec::new(),
        )?;
        self.app.remount(
            Id::ConfigEditor(IdConfigEditor::Footer),
            Box::new(CEFooter::new(&self.config_tui.read())),
            Vec::new(),
        )?;

        Ok(())
    }

    /// Unmount the Config-Editor's Header & Footer
    fn umount_config_header_footer(&mut self) -> Result<()> {
        self.app.umount(&Id::ConfigEditor(IdConfigEditor::Header))?;

        self.app.umount(&Id::ConfigEditor(IdConfigEditor::Footer))?;

        Ok(())
    }
}
