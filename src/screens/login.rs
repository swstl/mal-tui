use super::{BackgroundUpdate, ExtraInfo, Screen, screens::*, widgets::navigatable::Navigatable};
use crate::app::Action;
use crate::{
    add_screen_caching,
    app::Event,
    config::{Config, navigation::NavDirection},
    mal::MalClient,
    screens::widgets::button::Button,
};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::cmp::{max, min};
use std::thread::JoinHandle;

//TODO: option to copy the url to clipboard
#[derive(Clone)]
pub struct LoginScreen {
    full_url: String,
    buttons: Vec<&'static str>,
    login_url: String,
    app_info: ExtraInfo,
    navigatable: Navigatable,
}

impl LoginScreen {
    pub fn new(info: ExtraInfo) -> Self {
        Self {
            buttons: vec!["Open Browser", "Back"],
            full_url: String::new(),
            login_url: String::new(),
            app_info: info,
            navigatable: Navigatable::new((2, 1)),
        }
    }

    fn activate_button(&mut self, index: usize) -> Option<Action> {
        match index {
            0 => {
                if self.full_url.is_empty() {
                    return None;
                }

                if let Err(e) = open::that(&self.full_url) {
                    return Some(Action::ShowError(e.to_string()));
                }

                None
            }
            1 => {
                if MalClient::user_is_logged_in() {
                    self.login_url.clear();
                }
                Some(Action::SwitchScreen(LAUNCH))
            }
            _ => None,
        }
    }
}

impl Screen for LoginScreen {
    add_screen_caching!();

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        frame.render_widget(Clear, area);

        let page_chunk = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let header_text = vec![
            r"                                  #     %%%                                     ",
            r"                                @&@#   #%% #&                                   ",
            r"                   @&       &&#&%@@%  %@&&#@@#                                  ",
            r"                 @@##&&&@@ &&@;&%#   #|@#&#&@% ##                               ",
            r"              #%&&@&%%# &@   %|#&@%|~  %@###@&&%                                ",
            r"                &&@%@%@#&%  / @&%@%#   ##%%#% @&@                               ",
            r"         &@ #@#|&@%~%###&&##@#&@   #  __:_&%@  %##                              ",
            r"           @&@%#@ @ %%|&_@#%#      #:_     |%#  &@                              ",
            r"          @@#@@|@  &~/    @%&~    /     &&@@__#@%&#                             ",
            r"         %#%%@%#\&   :   #  % \\\/         |@&%&&                               ",
            r"          @ ##@#%&__ |      @   |          % @&  @                              ",
            r"        %@@  &&&@@# _~_=         |           #&                                 ",
            r"          &&%%& %# @    \=        =                                             ",
            r"        ##@~\;& &#       /:_::;_____                                            ",
            r"      %&## @#   \__=  //=          \;                                     @     ",
            r"       #    &       =_               \\          ________=             @&&%@@%# ",
            r"                                       \\    _~_:____     ___~          #@;%@%@ ",
            r"                                         =~_~;_~:              \       # |##%&# ",
            r"                                         =_\=                    ___: /:~_;#%&&@",
            r"                                         ~||                     |   =|   &&@%@@",
            r"                                         |;|                      :    |  @   &%",
            r"                                        |;|              &#@%    | \|%;%%       ",
            r"                                        |~|                %@  _| @&##% @@%%    ",
            r"                                        ;:=                  &%& @&%%&@%&@&     ",
            r"                                       |||                  &&%%#@&#&%%@# @     ",
            r"                           \__.-.____./~=|\..________/         @#&@#@@&&##      ",
            r"                            \         *    *..      /           &  @&%@&        ",
            r"                             \_____________________/                            ",
            r"                               ‾                 ‾                              ",
        ];

        let alpha = Paragraph::new(header_text.join("\n"))
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);

        frame.render_widget(alpha, page_chunk[0]);

        let text_field_area = Rect::new(
            page_chunk[1].x + min(page_chunk[1].width / 2 - 25, page_chunk[1].width / 4),
            page_chunk[1].y + 2,
            max(page_chunk[1].width / 2, 50),
            3,
        );

        let url_field = Paragraph::new(self.login_url.clone())
            .block(Block::default().borders(Borders::ALL))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);

        frame.render_widget(url_field, text_field_area);

        let button_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Length(6),
                Constraint::Fill(1),
            ])
            .split(page_chunk[1]);

        let button_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(20),
                Constraint::Fill(1),
            ])
            .split(button_area[1]);

        self.navigatable
            .construct(&self.buttons, button_area[1], |button, area, iselected| {
                Button::new(button).selected(iselected).render(frame, area);
            });
    }

    fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        let nav = &Config::global().navigation;

        match nav.get_direction(&key_event.code) {
            NavDirection::Up => {
                self.navigatable.move_up();
            }
            NavDirection::Down => self.navigatable.move_down(),
            _ => {}
        };

        if nav.is_select(&key_event.code) {
            return self.activate_button(self.navigatable.get_selected_index());
        }
        None
    }

    fn handle_mouse(&mut self, mouse_event: crossterm::event::MouseEvent) -> Option<Action> {
        if let Some(index) = self.navigatable.get_hovered_index(mouse_event)
            && let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind
        {
            return self.activate_button(index);
        };

        None
    }

    fn background(&mut self) -> Option<JoinHandle<()>> {
        if MalClient::user_is_logged_in() {
            return None;
        }

        let login_url = self.login_url.clone();
        let id = self.get_name();
        let info = self.app_info.clone();
        let mal_client = info.mal_client.clone();

        Some(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            {
                if !login_url.is_empty() {
                    return;
                }
            }

            let (url_to_print, joinable) = MalClient::init_oauth();

            // the full url
            let update = BackgroundUpdate::new(id.clone()).set("full_url", url_to_print.clone());
            let _ = info.app_sx.send(Event::BackgroundNotice(update));

            // for the printing effect
            for i in 0..url_to_print.len() + 1 {
                let new_url = url_to_print[0..i].to_string();
                let update = BackgroundUpdate::new(id.clone()).set("login_url", new_url);
                let _ = info.app_sx.send(Event::BackgroundNotice(update));
                std::thread::sleep(std::time::Duration::from_millis(8));
            }

            joinable.join().unwrap();
            mal_client.update_user_login();
            let new_url = "Login successful".to_string();
            let update = BackgroundUpdate::new(id.clone()).set("login_url", new_url);
            let _ = info.app_sx.send(Event::BackgroundNotice(update));
        }))
    }

    fn apply_update(&mut self, update: BackgroundUpdate) {
        if let Some(url) = update.get::<String>("login_url") {
            self.login_url = url.clone();
        }
        if let Some(url) = update.get::<String>("full_url") {
            self.full_url = url.clone();
        }
    }

    fn uses_navbar(&self) -> bool {
        false
    }
}
