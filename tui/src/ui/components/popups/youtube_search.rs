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
use crate::ui::model::{Model, UserEvent};
use termusiclib::config::{SharedTuiSettings, TuiOverlay};
use termusiclib::ids::Id;
use termusiclib::types::{Msg, YSMsg};
use tui_realm_stdlib::{Input, Table};
use tuirealm::command::{Cmd, CmdResult, Direction, Position};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{Alignment, BorderType, Borders, InputType, TableBuilder, TextSpan};
use tuirealm::{Component, Event, MockComponent, State, StateValue};

#[derive(MockComponent)]
pub struct YSInputPopup {
    component: Input,
}

impl YSInputPopup {
    pub fn new(config: &TuiOverlay) -> Self {
        let settings = &config.settings;
        Self {
            component: Input::default()
                .background(settings.theme.fallback_background())
                .foreground(settings.theme.fallback_foreground())
                .borders(
                    Borders::default()
                        .color(settings.theme.fallback_border())
                        .modifiers(BorderType::Rounded),
                )
                // .invalid_style(Style::default().fg(Color::Red))
                .input_type(InputType::Text)
                .title(" Download url or search: ", Alignment::Left),
        }
    }
}

impl Component<Msg, UserEvent> for YSInputPopup {
    fn on(&mut self, ev: Event<UserEvent>) -> Option<Msg> {
        let cmd_result = match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Left, ..
            }) => self.perform(Cmd::Move(Direction::Left)),
            Event::Keyboard(KeyEvent {
                code: Key::Right, ..
            }) => self.perform(Cmd::Move(Direction::Right)),
            Event::Keyboard(KeyEvent {
                code: Key::Home, ..
            }) => self.perform(Cmd::GoTo(Position::Begin)),
            Event::Keyboard(KeyEvent { code: Key::End, .. }) => {
                self.perform(Cmd::GoTo(Position::End))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Delete, ..
            }) => self.perform(Cmd::Cancel),
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) => self.perform(Cmd::Delete),
            Event::Keyboard(KeyEvent {
                code: Key::Char(ch),
                modifiers: KeyModifiers::SHIFT | KeyModifiers::NONE,
            }) => self.perform(Cmd::Type(ch)),
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                return Some(Msg::YoutubeSearch(YSMsg::InputPopupCloseCancel));
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => self.perform(Cmd::Submit),
            _ => CmdResult::None,
        };
        match cmd_result {
            CmdResult::Submit(State::One(StateValue::String(input_string))) => {
                Some(Msg::YoutubeSearch(YSMsg::InputPopupCloseOk(input_string)))
            }

            CmdResult::None => None,
            _ => Some(Msg::ForceRedraw),
        }
    }
}

#[derive(MockComponent)]
pub struct YSTablePopup {
    component: Table,
    config: SharedTuiSettings,
}

impl YSTablePopup {
    pub fn new(config: SharedTuiSettings) -> Self {
        let component = {
            let config = config.read();
            Table::default()
                .background(config.settings.theme.fallback_background())
                .foreground(config.settings.theme.fallback_foreground())
                .borders(
                    Borders::default()
                        .color(config.settings.theme.fallback_border())
                        .modifiers(BorderType::Rounded),
                )
                // .foreground(Color::Yellow)
                .title(
                    " Tab/Shift+Tab for next and previous page ",
                    Alignment::Left,
                )
                .scroll(true)
                .highlighted_color(config.settings.theme.fallback_highlight())
                .highlighted_str(&config.settings.theme.style.library.highlight_symbol)
                // .highlighted_str("🚀")
                .rewind(false)
                .step(4)
                .row_height(1)
                .headers(["Duration", "Name"])
                .column_spacing(3)
                .widths(&[20, 80])
                .table(
                    TableBuilder::default()
                        .add_col(TextSpan::from("Empty result."))
                        .add_col(TextSpan::from("Loading..."))
                        .build(),
                )
        };

        Self { component, config }
    }
}

impl Component<Msg, UserEvent> for YSTablePopup {
    fn on(&mut self, ev: Event<UserEvent>) -> Option<Msg> {
        let config = self.config.clone();
        let keys = &config.read().settings.keys;
        let cmd_result = match ev {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                return Some(Msg::YoutubeSearch(YSMsg::TablePopupCloseCancel));
            }
            Event::Keyboard(keyevent) if keyevent == keys.quit.get() => {
                return Some(Msg::YoutubeSearch(YSMsg::TablePopupCloseCancel));
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                self.perform(Cmd::Move(Direction::Up))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => self.perform(Cmd::Move(Direction::Down)),

            Event::Keyboard(keyevent) if keyevent == keys.navigation_keys.down.get() => {
                self.perform(Cmd::Move(Direction::Down))
            }

            Event::Keyboard(keyevent) if keyevent == keys.navigation_keys.up.get() => {
                self.perform(Cmd::Move(Direction::Up))
            }
            Event::Keyboard(KeyEvent {
                code: Key::PageDown,
                ..
            }) => self.perform(Cmd::Scroll(Direction::Down)),
            Event::Keyboard(KeyEvent {
                code: Key::PageUp, ..
            }) => self.perform(Cmd::Scroll(Direction::Up)),
            Event::Keyboard(keyevent) if keyevent == keys.navigation_keys.goto_top.get() => {
                self.perform(Cmd::GoTo(Position::Begin))
            }
            Event::Keyboard(keyevent) if keyevent == keys.navigation_keys.goto_bottom.get() => {
                self.perform(Cmd::GoTo(Position::End))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Tab,
                modifiers: KeyModifiers::NONE,
            }) => return Some(Msg::YoutubeSearch(YSMsg::TablePopupNext)),
            Event::Keyboard(KeyEvent {
                code: Key::BackTab,
                modifiers: KeyModifiers::SHIFT,
            }) => return Some(Msg::YoutubeSearch(YSMsg::TablePopupPrevious)),
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => {
                if let State::One(StateValue::Usize(index)) = self.state() {
                    return Some(Msg::YoutubeSearch(YSMsg::TablePopupCloseOk(index)));
                }
                CmdResult::None
            }
            _ => CmdResult::None,
        };
        match cmd_result {
            CmdResult::None => None,
            _ => Some(Msg::ForceRedraw),
        }
    }
}

impl Model {
    pub fn mount_youtube_search_input(&mut self) {
        assert!(
            self.app
                .remount(
                    Id::YoutubeSearchInputPopup,
                    Box::new(YSInputPopup::new(&self.config_tui.read())),
                    vec![]
                )
                .is_ok()
        );
        assert!(self.app.active(&Id::YoutubeSearchInputPopup).is_ok());
    }

    pub fn mount_youtube_search_table(&mut self) {
        assert!(
            self.app
                .remount(
                    Id::YoutubeSearchTablePopup,
                    Box::new(YSTablePopup::new(self.config_tui.clone())),
                    vec![]
                )
                .is_ok()
        );
        assert!(self.app.active(&Id::YoutubeSearchTablePopup).is_ok());
        if let Err(e) = self.update_photo() {
            self.mount_error_popup(e.context("update_photo"));
        }
    }

    pub fn umount_youtube_search_table_popup(&mut self) {
        if self.app.mounted(&Id::YoutubeSearchTablePopup) {
            assert!(self.app.umount(&Id::YoutubeSearchTablePopup).is_ok());
        }
        if let Err(e) = self.update_photo() {
            self.mount_error_popup(e.context("update_photo"));
        }
    }
}
