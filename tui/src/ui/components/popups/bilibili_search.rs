use termusiclib::config::{SharedTuiSettings, TuiOverlay};
use tui_realm_stdlib::Table;
use tuirealm::command::{Cmd, CmdResult, Direction, Position};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{Alignment, BorderType, Borders, InputType, TableBuilder, TextSpan};
use tuirealm::{Component, Event, MockComponent, State, StateValue};

use crate::ui::components::vendored::tui_realm_stdlib_input::Input;
use crate::ui::ids::Id;
use crate::ui::model::{Model, UserEvent};
use crate::ui::msg::{BSMsg, Msg};

#[derive(MockComponent)]
pub struct BSInputPopup {
    component: Input,
}

impl BSInputPopup {
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
                .input_type(InputType::Text)
                .title(" Download url or search (Bilibili): ", Alignment::Left),
        }
    }
}

impl Component<Msg, UserEvent> for BSInputPopup {
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
                return Some(Msg::BilibiliSearch(BSMsg::InputPopupCloseCancel));
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => self.perform(Cmd::Submit),
            _ => CmdResult::None,
        };
        match cmd_result {
            CmdResult::Submit(State::One(StateValue::String(input_string))) => {
                Some(Msg::BilibiliSearch(BSMsg::InputPopupCloseOk(input_string)))
            }

            CmdResult::None => None,
            _ => Some(Msg::ForceRedraw),
        }
    }
}

#[derive(MockComponent)]
pub struct BSTablePopup {
    component: Table,
    config: SharedTuiSettings,
}

impl BSTablePopup {
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
                .title(
                    " Tab/Shift+Tab for next and previous page ",
                    Alignment::Left,
                )
                .scroll(true)
                .highlighted_color(config.settings.theme.fallback_highlight())
                .highlighted_str(&config.settings.theme.style.library.highlight_symbol)
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

impl Component<Msg, UserEvent> for BSTablePopup {
    fn on(&mut self, ev: Event<UserEvent>) -> Option<Msg> {
        let config = self.config.clone();
        let keys = &config.read().settings.keys;
        let cmd_result = match ev {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                return Some(Msg::BilibiliSearch(BSMsg::TablePopupCloseCancel));
            }
            Event::Keyboard(keyevent) if keyevent == keys.quit.get() => {
                return Some(Msg::BilibiliSearch(BSMsg::TablePopupCloseCancel));
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
            Event::Keyboard(KeyEvent { code: Key::Home, .. }) => {
                self.perform(Cmd::GoTo(Position::Begin))
            }
            Event::Keyboard(KeyEvent { code: Key::End, .. }) => {
                self.perform(Cmd::GoTo(Position::End))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Tab,
                modifiers: KeyModifiers::NONE,
            }) => return Some(Msg::BilibiliSearch(BSMsg::ReqNextPage)),
            Event::Keyboard(KeyEvent {
                code: Key::BackTab,
                modifiers: KeyModifiers::SHIFT,
            }) => return Some(Msg::BilibiliSearch(BSMsg::ReqPreviousPage)),
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => {
                if let State::One(StateValue::Usize(index)) = self.state() {
                    return Some(Msg::BilibiliSearch(BSMsg::TablePopupCloseOk(index)));
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
    pub fn mount_bilibili_search_input(&mut self) {
        self.app
            .remount(
                Id::BilibiliSearchInputPopup,
                Box::new(BSInputPopup::new(&self.config_tui.read())),
                vec![],
            )
            .ok();
        self.app.active(&Id::BilibiliSearchInputPopup).ok();
    }

    pub fn mount_bilibili_search_table(&mut self) {
        self.app
            .remount(
                Id::BilibiliSearchTablePopup,
                Box::new(BSTablePopup::new(self.config_tui.clone())),
                vec![],
            )
            .ok();
        self.app.active(&Id::BilibiliSearchTablePopup).ok();
        if let Err(e) = self.update_photo() {
            self.mount_error_popup(e.context("update_photo"));
        }
    }

    pub fn umount_bilibili_search_table_popup(&mut self) {
        if self.app.mounted(&Id::BilibiliSearchTablePopup) {
            self.app.umount(&Id::BilibiliSearchTablePopup).ok();
        }
        if let Err(e) = self.update_photo() {
            self.mount_error_popup(e.context("update_photo"));
        }
    }
}
