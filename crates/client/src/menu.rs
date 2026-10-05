//! Start screen for the wasm build: host a room, join one by code, or
//! play locally. Native builds never see this module — they start in the
//! local game or connect to a server via `--server`.

use macroquad::prelude::*;

use crate::net::{NetEvent, NetHandle};
use crate::{
    COPIED_TOAST, GOLD, Mode, MUTED, Online, PANEL, PANEL_EDGE, WHITE, draw_button,
    draw_round_rect, draw_toast, expire_toast, new_local_game, point_in_rect,
};

const BUTTON_WIDTH: f32 = 280.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 18.0;
const MAX_CODE_LEN: usize = 8;

enum View {
    Choose,
    JoinCode,
}

pub struct Menu {
    net: NetHandle,
    view: View,
    code: String,
    /// Set between clicking HOST GAME and the bridge confirming the room.
    awaiting_room: bool,
    toast: Option<(String, f64)>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            net: NetHandle,
            view: View::Choose,
            code: String::new(),
            awaiting_room: false,
            toast: None,
        }
    }

    pub fn with_toast(message: String) -> Self {
        let mut menu = Self::new();
        menu.toast = Some((message, get_time() + 6.0));
        menu
    }

    /// Pumps bridge events and input; returns the mode to switch to.
    pub fn update(&mut self) -> Option<Mode> {
        while let Some(event) = self.net.poll() {
            match event {
                // The page URL carried a room code: join it right away.
                NetEvent::Booted { room: Some(code) } => {
                    self.net.join_room(code.clone());
                    return Some(Mode::Online(Online::joining(code)));
                }
                NetEvent::Booted { room: None } => {}
                NetEvent::Room { code, share_url } => {
                    self.awaiting_room = false;
                    return Some(Mode::Online(Online::hosted(code, share_url)));
                }
                NetEvent::Copied => {
                    self.toast = Some((COPIED_TOAST.to_owned(), get_time() + 3.0));
                }
                _ => {}
            }
        }

        expire_toast(&mut self.toast);
        self.handle_input()
    }

    fn handle_input(&mut self) -> Option<Mode> {
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let mouse: Vec2 = mouse_position().into();

        match self.view {
            View::Choose => {
                if clicked {
                    let layout = choose_layout();
                    if point_in_rect(mouse, layout.host) {
                        self.awaiting_room = true;
                        self.net.host_room();
                    } else if point_in_rect(mouse, layout.join) {
                        self.view = View::JoinCode;
                    } else if point_in_rect(mouse, layout.local) {
                        return Some(Mode::Local {
                            game: new_local_game(),
                        });
                    }
                }
                None
            }
            View::JoinCode => {
                self.type_code();
                if is_key_pressed(KeyCode::Escape) {
                    self.view = View::Choose;
                }

                let layout = join_layout();
                let pressed_join =
                    is_key_pressed(KeyCode::Enter) || (clicked && point_in_rect(mouse, layout.join));
                let pressed_back = clicked && point_in_rect(mouse, layout.back);
                if pressed_back {
                    self.view = View::Choose;
                } else if pressed_join && !self.code.is_empty() {
                    let code = std::mem::take(&mut self.code);
                    self.net.join_room(code.clone());
                    return Some(Mode::Online(Online::joining(code)));
                }
                None
            }
        }
    }

    fn type_code(&mut self) {
        while let Some(character) = get_char_pressed() {
            if character.is_ascii_alphanumeric() && self.code.len() < MAX_CODE_LEN {
                self.code.push(character.to_ascii_uppercase());
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.code.pop();
        }
    }

    pub fn draw(&self) {
        match self.view {
            View::Choose => self.draw_choose(),
            View::JoinCode => self.draw_join(),
        }

        if let Some((text, expires)) = &self.toast {
            draw_toast(text, *expires);
        }
    }

    fn draw_choose(&self) {
        draw_text("HIBE DI HOPEDI", 26.0, 48.0, 34.0, WHITE);
        draw_text("A forced-hop territory prototype", 28.0, 72.0, 15.0, MUTED);

        let layout = choose_layout();
        draw_button(layout.host, "HOST GAME", GOLD);
        draw_button(layout.join, "JOIN WITH CODE", PANEL_EDGE);
        draw_button(layout.local, "PLAY LOCALLY", PANEL_EDGE);

        for (rect, hint) in [
            (&layout.host, "create a room and share the link"),
            (&layout.join, "enter a code from your friend"),
            (&layout.local, "two players on this device"),
        ] {
            let width = measure_text(hint, None, 14, 1.0).width;
            draw_text(hint, rect.x + (rect.w - width) / 2.0, rect.y + rect.h + 18.0, 14.0, MUTED);
        }

        let footer = "online play is peer-to-peer: the first player hosts the room.";
        let width = measure_text(footer, None, 14, 1.0).width;
        draw_text(
            footer,
            (screen_width() - width) / 2.0,
            screen_height() - 34.0,
            14.0,
            MUTED,
        );
    }

    fn draw_join(&self) {
        let title = "JOIN A ROOM";
        let title_width = measure_text(title, None, 26, 1.0).width;
        draw_text(title, (screen_width() - title_width) / 2.0, 130.0, 26.0, WHITE);

        let layout = join_layout();
        draw_round_rect(layout.input.x, layout.input.y, layout.input.w, layout.input.h, 12.0, PANEL);
        draw_rectangle_lines(layout.input.x, layout.input.y, layout.input.w, layout.input.h, 12.0, PANEL_EDGE);

        let shown = if self.code.is_empty() {
            "···".to_owned()
        } else {
            format!("{}_", self.code)
        };
        let code_width = measure_text(&shown, None, 34, 1.0).width;
        draw_text(
            &shown,
            layout.input.x + (layout.input.w - code_width) / 2.0,
            layout.input.y + 52.0,
            34.0,
            if self.code.is_empty() { MUTED } else { WHITE },
        );

        draw_button(layout.join, "JOIN", GOLD);
        draw_button(layout.back, "BACK", PANEL_EDGE);

        let hint = "type the code your friend shared, then press enter";
        let width = measure_text(hint, None, 14, 1.0).width;
        draw_text(hint, (screen_width() - width) / 2.0, layout.input.y + 110.0, 14.0, MUTED);
    }
}

struct ChooseLayout {
    host: Rect,
    join: Rect,
    local: Rect,
}

fn choose_layout() -> ChooseLayout {
    let total = BUTTON_HEIGHT * 3.0 + BUTTON_GAP * 2.0;
    let top = screen_height() / 2.0 - total / 2.0 + 30.0;
    let x = (screen_width() - BUTTON_WIDTH) / 2.0;
    let rect = |index: usize| {
        Rect::new(
            x,
            top + (BUTTON_HEIGHT + BUTTON_GAP) * index as f32,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
        )
    };
    ChooseLayout {
        host: rect(0),
        join: rect(1),
        local: rect(2),
    }
}

struct JoinLayout {
    input: Rect,
    join: Rect,
    back: Rect,
}

fn join_layout() -> JoinLayout {
    let input_width = 300.0;
    let input = Rect::new(
        (screen_width() - input_width) / 2.0,
        screen_height() / 2.0 - 120.0,
        input_width,
        72.0,
    );
    let button_width = 130.0;
    let button_y = input.y + input.h + 40.0;
    JoinLayout {
        input,
        join: Rect::new(
            screen_width() / 2.0 - button_width - 10.0,
            button_y,
            button_width,
            BUTTON_HEIGHT,
        ),
        back: Rect::new(
            screen_width() / 2.0 + 10.0,
            button_y,
            button_width,
            BUTTON_HEIGHT,
        ),
    }
}
