use std::{collections::{HashMap, HashSet}, thread::JoinHandle};

use super::{
    ExtraInfo, Screen,
    screens::*,
    widgets::{button::Button, navigatable::Navigatable},
};
use crate::{app::Event, mal::{MalClient, models::anime::Anime}, screens::BackgroundUpdate, utils::functionStreaming::StreamableRunner};
use crate::{
    app::Action,
    config::{Config, navigation::NavDirection},
};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Clear, Paragraph},
};

pub struct LaunchScreen {
    buttons: Vec<&'static str>,
    navigatable: Navigatable,
    app_info: ExtraInfo,
}

impl LaunchScreen {
    pub fn new(app_info: ExtraInfo) -> Self {
        Self {
            buttons: vec![
                "Browse",
                if !MalClient::user_is_logged_in() {
                    "Log In"
                } else {
                    "Log Out"
                },
                "Exit",
            ],
            navigatable: Navigatable::new((3, 1)),
            app_info,
        }
    }

    fn activate_button(&self, index: usize) -> Option<Action> {
        match index {
            0 => Some(Action::SwitchScreen(OVERVIEW)),
            1 => {
                if MalClient::user_is_logged_in() {
                    MalClient::log_out();
                    Some(Action::SwitchScreen(LAUNCH))
                } else {
                    Some(Action::SwitchScreen(LOGIN))
                }
            }
            2 => Some(Action::Quit),
            _ => None,
        }
    }
}

impl Screen for LaunchScreen {
    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        frame.render_widget(Clear, area);

        let page_chunk = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let button_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(20),
                Constraint::Fill(1),
            ])
            .split(page_chunk[1]);

        let button_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Length((self.buttons.len() * 3) as u16),
                Constraint::Percentage(80),
            ])
            .split(button_area[1]);

        let centeded_chunk = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Percentage(30)])
            .split(page_chunk[0]);

        let header_text = [
            " ███╗   ███╗ █████╗ ██╗              ████████╗██╗   ██╗██╗ ",
            " ████╗ ████║██╔══██╗██║              ╚══██╔══╝██║   ██║██║ ",
            " ██╔████╔██║███████║██║     ███████╗    ██║   ██║   ██║██║ ",
            " ██║╚██╔╝██║██╔══██║██║     ╚══════╝    ██║   ██║   ██║██║ ",
            " ██║ ╚═╝ ██║██║  ██║███████╗            ██║   ╚██████╔╝██║ ",
            " ╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝            ╚═╝    ╚═════╝ ╚═╝ ",
        ];

        let alpha = Paragraph::new(header_text.join("\n"))
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);

        frame.render_widget(alpha, centeded_chunk[1]);

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
            NavDirection::Down => {
                self.navigatable.move_down();
            }
            _ => {}
        };

        if nav.is_select(&key_event.code) {
            return self.activate_button(self.navigatable.get_selected_index());
        }

        None
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<Action> {
        if let Some(index) = self.navigatable.get_hovered_index(mouse_event)
            && let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind
        {
            return self.activate_button(index);
        };

        None
    }

    fn uses_navbar(&self) -> bool {
        false
    }

    fn background(&mut self) -> Option<JoinHandle<()>> {

        let info = self.app_info.clone();
        let id = self.get_name();

        Some(std::thread::spawn(move || {
            ////////////////////////////////////////
            //////// Sync local db with MAL ////////
            ////////////////////////////////////////
            let mut animes_to_sync: Vec<Anime> = vec![];
            let local_animes = info.local_db.get::<Anime>(None).unwrap_or_default();
            let local_lookup: HashMap<_, _> = local_animes
                .iter()
                .map(|a| (a.id, a))
                .collect();

            let mut checked_ids = HashSet::new();

            let anime_generator = StreamableRunner::new()
                .with_batch_size(1000)
                .stop_early()
                .stop_at(20);

            for animes in anime_generator
                .run(|offset, limit| info.mal_client.get_anime_list(None, offset, limit))
            {
                for anime in animes.iter() {
                    checked_ids.insert(anime.id);

                    match local_lookup.get(&anime.id) {
                        Some(&local_anime) => {
                            if anime.my_list_status.status != local_anime.my_list_status.status
                                || anime.my_list_status.score != local_anime.my_list_status.score
                                || anime.my_list_status.num_episodes_watched != local_anime.my_list_status.num_episodes_watched
                            {
                                animes_to_sync.push(local_anime.clone());
                            }
                        }
                        None => animes_to_sync.push(anime.clone()),
                    }
                }
                let update = BackgroundUpdate::new(id.clone())
                    .set("animes", animes);
                info.app_sx.send(Event::BackgroundNotice(update)).ok();
            }

            for local_anime in local_animes.iter() {
                if !checked_ids.contains(&local_anime.id) {
                    animes_to_sync.push(local_anime.clone());
                }
            }

            let update = BackgroundUpdate::new(id.clone())
                .set("sync", animes_to_sync);
            info.app_sx.send(Event::BackgroundNotice(update)).ok();
        }))
    }
}
