use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
    thread::JoinHandle,
};

use crate::{
    app::{Action, Event},
    config::{Config, navigation::NavDirection},
    mal::{
        MalClient,
        models::anime::{Anime, AnimeId, DeleteOrUpdate, MyListStatus, status_is_known},
    },
    screens::{BackgroundUpdate, ExtraInfo},
    send_error,
    utils::{
        imageManager::ImageManager,
        stringManipulation::{DisplayString, format_date},
        terminalCapabilities::TERMINAL_RATIO,
    },
};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Position, Rect},
    style::{Color, Style, Stylize},
    symbols::{self, border},
    widgets::{
        Block, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Wrap,
    },
};
use std::cmp::min;
use tui_widgets::big_text::{BigText, PixelSize};

use super::{infobox::InfoBox, navigatable::Navigatable};

const AVAILABLE_SEASONS: [&str; 4] = ["Winter", "Spring", "Summer", "Fall"];
const FIRST_YEAR: u16 = 1917;
const FIRST_SEASON: &str = "Winter";
const BUTTON_HEIGHT: u16 = 3;
const RATIO: f32 = 422.0 / 598.0;

#[derive(PartialEq, Clone, Debug)]
enum Focus {
    PlayButtons,
    StatusButtons,
    Synopsis,
}

// #[derive(PartialEq, Clone, Debug)]
enum LocalEvent {
    UserChoice(usize, Anime),
    ExtraInfo(Anime),
}

#[derive(Clone)]
pub struct AnimePopup {
    untogglable: bool,
    anime_id: AnimeId,
    toggled: bool,
    buttons: Vec<String>,
    button_nav: Navigatable,
    status_buttons: Vec<SelectionPopup>,
    status_nav: Navigatable,
    image_manager: Arc<Mutex<ImageManager>>,
    focus: Focus,
    background_transmitter: Sender<LocalEvent>,
    app_info: ExtraInfo,
    synopsis_scroll: u16,

    //cache
    popup_area: Option<Rect>,
    synopsis_area: Option<Rect>,
}

impl AnimePopup {
    pub fn new(info: ExtraInfo) -> Self {
        let buttons = vec![
            "Play".to_string(),
            "nothing yet".to_string(),
            "Play from start".to_string(),
            "Open".to_string(),
        ];
        let image_manager = Arc::new(Mutex::new(ImageManager::new()));
        let (tx, rx) = std::sync::mpsc::channel::<LocalEvent>();

        ImageManager::init_with_threads(&image_manager, info.app_sx.clone());

        let popup = Self {
            untogglable: false,
            app_info: info.clone(),
            image_manager,
            anime_id: AnimeId::default(),
            toggled: false,
            button_nav: Navigatable::new((buttons.len() as u16, 1)),
            status_nav: Navigatable::new((1, 3)),
            status_buttons: Vec::new(),
            buttons,
            focus: Focus::PlayButtons,
            background_transmitter: tx,
            synopsis_scroll: 0,
            popup_area: None,
            synopsis_area: None,
        };
        popup.spawn_background(info, rx);
        popup
    }

    fn spawn_background(
        &self,
        info: ExtraInfo,
        reveicer: Receiver<LocalEvent>,
    ) -> Option<JoinHandle<()>> {
        let mal_client = info.mal_client.clone();
        let app_sx = info.app_sx.clone();
        Some(std::thread::spawn(move || {
            while let Ok(event) = reveicer.recv() {
                match event {
                    // send any userchoice to the mal backend
                    LocalEvent::UserChoice(index, anime) => {
                        match info.mal_client.update_user_list(anime) {
                            Ok(result) => {
                                let update = BackgroundUpdate::new("popup")
                                    .set("success", (index, result.clone()));
                                info.app_sx.send(Event::BackgroundNotice(update)).ok();
                            }
                            Err(e) => {
                                info.app_sx
                                    .send(Event::BackgroundNotice(
                                        BackgroundUpdate::new("popup")
                                            .set("failure", (index, e.to_string())),
                                    ))
                                    .ok();
                            }
                        }
                    }

                    // update the number of released episodes
                    LocalEvent::ExtraInfo(anime) => {
                        let available_episodes =
                            mal_client.get_available_episodes(anime.id).unwrap_or(None);
                        if let Some(episodes) = available_episodes {
                            app_sx
                                .send(Event::StorageUpdate(
                                    anime.id,
                                    Box::new(move |anime: &mut Anime| {
                                        anime.num_released_episodes = Some(episodes);
                                        if anime.num_episodes == 0 {
                                            anime.num_episodes = episodes;
                                            anime.episode_count_ready = false;
                                        }
                                    }),
                                ))
                                .unwrap();
                        }
                    }
                }
            }
        }))
    }

    // TODO: then this is not needed
    pub fn apply_update(&mut self, mut update: BackgroundUpdate) {
        if let Some((index, (_, update))) =
            update.take::<(usize, (usize, DeleteOrUpdate))>("success")
        {
            self.app_info.anime_store.update(self.anime_id, |anime| {
                anime.my_list_status = match update {
                    DeleteOrUpdate::Deleted(_vec) => MyListStatus::default(),
                    DeleteOrUpdate::Updated(status) => status,
                }
            });

            if let Some(button) = self
                .status_nav
                .get_item_at_index_mut(&mut self.status_buttons, index)
                && let Some(option) = button.get_selected_option()
            {
                button.set_color(Config::global().theme.status_color(option));
            };
        }

        if let Some(index) = update.take::<usize>("failure")
            && let Some(button) = self
                .status_nav
                .get_item_at_index_mut(&mut self.status_buttons, index)
        {
            button.set_color(Config::global().theme.error);
        }

        self.update_buttons();
    }

    pub fn set_play_button_episode(&mut self, episode: Option<u32>) -> &Self {
        // if an anime is given set the button to its episode
        if let Some(episode) = episode {
            self.buttons[0] = format!("Play ▶ (EP {})", episode);
            return self;
        }

        // if no aniem is set use the current anime of the popup
        let anime = self
            .app_info
            .anime_store
            .get(&self.anime_id)
            .expect("(buttons) unexpected anime id given");

        // if the anime has no episodes set the button to "no episodes"
        if anime.num_episodes == 0 {
            self.buttons[0] = "No episodes".to_string();

        // noraml case
        } else {
            self.buttons[0] = format!(
                "Play ▶ (EP {})",
                (anime.my_list_status.num_episodes_watched + 1).min(anime.num_episodes)
            );
        }

        // if the anime has released episodes and the next episode to play is higher than the available episodes
        if let Some(available_episodes) = anime.num_released_episodes {
            let episode_to_play =
                (anime.my_list_status.num_episodes_watched + 1).min(anime.num_episodes);
            if episode_to_play > available_episodes {
                self.buttons[0] = format!("Try to play (EP {})", episode_to_play)
            }
        }
        self
    }
    pub fn update_buttons(&mut self) -> &Self {
        let anime = match self.app_info.anime_store.get(&self.anime_id) {
            Some(anime) => anime,
            None => {
                return self;
            }
        };

        self.set_play_button_episode(None);
        let episode_options: Vec<String> = (0..=anime.num_episodes.max(1))
            .map(|i| i.to_string())
            .collect();

        self.status_buttons = vec![
            SelectionPopup::new()
                .add_option("Add to list")
                .add_option("Watching")
                .add_option("Plan to watch")
                .add_option("Completed")
                .add_option("On Hold")
                .add_option("Dropped")
                .with_color(
                    Config::global()
                        .theme
                        .status_color(&anime.my_list_status.status),
                )
                .with_arrows(Arrows::Static)
                .with_selected_option(anime.my_list_status.status.to_string())
                .clone(),
            SelectionPopup::new()
                .add_option("Not rated")
                .add_options(vec!["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"])
                .with_arrows(Arrows::Static)
                .with_selected_option(anime.my_list_status.score.to_string())
                .clone(),
            SelectionPopup::new()
                .add_options(episode_options)
                .with_arrows(Arrows::Static)
                .with_selected_option(anime.my_list_status.num_episodes_watched.to_string())
                .with_displaying_format(format!("{{}} / {}", anime.num_episodes))
                .clone(),
        ];
        self
    }

    pub fn set_anime(&mut self, anime_id: AnimeId) -> &Self {
        self.anime_id = anime_id;
        let anime = match self.app_info.anime_store.get(&self.anime_id) {
            Some(anime) => {
                self.untogglable = false;
                anime
            }
            None => {
                self.untogglable = true;
                return self;
            }
        };

        self.update_buttons();
        if anime.num_released_episodes.is_none() {
            self.background_transmitter
                .send(LocalEvent::ExtraInfo((*anime).clone()))
                .ok();
        }
        self
    }

    pub fn is_open(&self) -> bool {
        self.toggled
    }

    pub fn open(&mut self) -> &Self {
        if !self.untogglable {
            self.toggled = true;
        }
        self
    }

    pub fn close(&mut self) -> &Self {
        self.toggled = false;
        self
    }

    pub fn update_status(&mut self, selection: String, index: usize) {
        let mut anime = (*self
            .app_info
            .anime_store
            .get(&self.anime_id)
            .expect("(Focus) unexpected anime id given"))
        .clone();

        match index {
            0 => {
                anime.my_list_status.status = selection.to_lowercase().replace(" ", "_");
            }
            1 => {
                anime.my_list_status.score = selection.parse().unwrap_or(0);
            }
            2 => {
                anime.my_list_status.num_episodes_watched = selection.parse().unwrap_or(0);
                if !status_is_known(anime.my_list_status.status.clone())
                    && anime.my_list_status.num_episodes_watched == 0
                {
                    return;
                } else if !status_is_known(anime.my_list_status.status.clone()) {
                    anime.my_list_status.status = "watching".to_string();
                }
            }
            _ => return,
        }

        self.background_transmitter
            .send(LocalEvent::UserChoice(index, anime.clone()))
            .ok();

        self.set_play_button_episode(Some(
            (anime.my_list_status.num_episodes_watched + 1).min(anime.num_episodes),
        ));
    }

    pub fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        let nav = &Config::global().navigation;

        match self.focus {
            Focus::PlayButtons => {
                match nav.get_direction(&key_event.code) {
                    NavDirection::Down => {
                        self.button_nav.move_down();
                    }
                    NavDirection::Up => {
                        if self.button_nav.get_selected_index() == 0 {
                            self.focus = Focus::StatusButtons;
                        }
                        self.button_nav.move_up();
                    }
                    NavDirection::Left => {
                        self.focus = Focus::Synopsis;
                    }
                    _ => {}
                }

                if nav.is_select(&key_event.code) {
                    let button = self.button_nav.get_selected_index();
                    match button {
                        0 => {
                            // play normally
                            return Some(Action::PlayAnime(self.anime_id));
                        }
                        1 => {
                            // play a specific episode
                        }

                        2 => {
                            // play from start
                            return Some(Action::PlayEpisode(self.anime_id, 1));
                        }
                        3 => {
                            // open the anime page in the browser
                            match open::that(format!(
                                "https://myanimelist.net/anime/{}",
                                self.anime_id
                            )) {
                                Ok(_) => {}
                                Err(e) => {
                                    return Some(Action::ShowError(format!(
                                        "Failed to open anime page: {}",
                                        e
                                    )));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Focus::StatusButtons => {
                if let Some((dropdown, index)) = self
                    .status_nav
                    .get_selected_item_mut_and_index(&mut self.status_buttons)
                {
                    match (dropdown.is_open(), nav.get_direction(&key_event.code)) {
                        (false, NavDirection::Right) => {
                            self.status_nav.move_right();
                            return None;
                        }
                        (false, NavDirection::Left) => {
                            if self.status_nav.get_selected_index() == 0 {
                                self.focus = Focus::Synopsis;
                            }
                            self.status_nav.move_left();
                            return None;
                        }
                        (false, NavDirection::Down) => {
                            self.focus = Focus::PlayButtons;
                            if let Some(button) = self
                                .status_nav
                                .get_selected_item_mut(&mut self.status_buttons)
                            {
                                button.close();
                            }
                        }
                        (true, _) => {
                            if let Some(selection) = dropdown.handle_input(key_event) {
                                dropdown.set_color(Color::White);
                                self.update_status(selection, index);
                            }
                            return None;
                        }
                        _ => {
                            if let Some(selection) = dropdown.handle_input(key_event) {
                                dropdown.set_color(Color::White);
                                self.update_status(selection, index);
                                return None;
                            }
                            if nav.is_close(&key_event.code) {
                                self.close();
                                return None;
                            }
                        }
                    }
                }
            }

            Focus::Synopsis => match nav.get_direction(&key_event.code) {
                NavDirection::Down => {
                    self.synopsis_scroll = min(
                        self.synopsis_scroll + 1,
                        self.app_info
                            .anime_store
                            .get(&self.anime_id)
                            .unwrap()
                            .synopsis
                            .len() as u16,
                    );
                }
                NavDirection::Up => {
                    self.synopsis_scroll = self.synopsis_scroll.saturating_sub(1);
                }
                NavDirection::Right => {
                    self.focus = Focus::PlayButtons;
                }
                NavDirection::Left => {
                    self.focus = Focus::StatusButtons;
                }
                _ => {}
            },
        }

        if nav.is_close(&key_event.code) {
            self.close();
        }

        None
    }

    pub fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<Action> {
        let p_area = self.popup_area?;
        let pos = Position::new(mouse_event.column, mouse_event.row);
        let is_click = matches!(mouse_event.kind, MouseEventKind::Down(_));

        // the status buttons
        let dropdown = match self
            .status_nav
            .get_selected_item_mut(&mut self.status_buttons)
        {
            Some(d) if d.is_open() => Some(d),
            _ => self
                .status_nav
                .get_hovered_item_mut(&mut self.status_buttons, mouse_event),
        };

        if let Some(dropdown) = dropdown {
            self.focus = Focus::StatusButtons;
            if let Some(selection) = dropdown.handle_mouse(mouse_event) {
                dropdown.set_color(Color::White);
                let index = self.status_nav.get_selected_index();
                self.update_status(selection, index);
            };
            return None;
        }

        // the synopsis area
        if let Some(s_area) = self.synopsis_area {
            if s_area.contains(pos) {
                self.focus = Focus::Synopsis;
                match mouse_event.kind {
                    MouseEventKind::ScrollUp => {
                        self.synopsis_scroll = self.synopsis_scroll.saturating_sub(1);
                    }
                    MouseEventKind::ScrollDown => {
                        self.synopsis_scroll += 1;
                    }
                    _ => {}
                }
                return None;
            }
        }

        // close the whole popup
        if is_click && !p_area.contains(pos) {
            self.close();
            return None;
        }

        // the play buttons
        if self.button_nav.get_hovered_index(mouse_event).is_some() {
            self.focus = Focus::PlayButtons;
            if is_click {
                return self.handle_keyboard(KeyEvent::new(
                    KeyCode::Enter,
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            return None;
        };

        None
    }

    pub fn render(&mut self, frame: &mut Frame) {
        if !self.toggled {
            return;
        }

        let anime = self
            .app_info
            .anime_store
            .get(&self.anime_id)
            .expect("(render) unexpected anime id given");

        let area = frame.area();

        let [height, width] = [area.height * 8 / 10, area.width * 7 / 10];
        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );

        self.popup_area = Some(popup_area);

        // clear the space for the popup
        frame.render_widget(Clear, popup_area);

        // craete the border arond the whole popup
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .style(Style::default().fg(Config::global().theme.secondary));
        frame.render_widget(block, popup_area);

        // split the popup up so we can get the area for the bottons ont he right side
        let [left, right] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Percentage(30)])
            .areas(popup_area);
        //buttons area
        let [_, bottom_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(self.buttons.len() as u16 * BUTTON_HEIGHT + 1),
            ])
            .areas(right);

        // now create borders that makes the top and left connect to the rest
        let (right_set, right_border) = (
            symbols::border::Set {
                bottom_left: symbols::line::ROUNDED_BOTTOM_RIGHT,
                top_right: symbols::line::ROUNDED_BOTTOM_RIGHT,
                ..symbols::border::ROUNDED
            },
            Borders::ALL,
        );
        let right_block = Block::default()
            .borders(right_border)
            .border_set(right_set)
            .style(Style::default().fg(Config::global().theme.secondary));
        let buttons_area = Rect::new(
            bottom_area.x + 1,
            bottom_area.y + 1,
            bottom_area.width.saturating_sub(1),
            bottom_area.height.saturating_sub(1),
        );
        frame.render_widget(right_block, bottom_area);

        // add the buttons
        self.button_nav
            .construct(&self.buttons, buttons_area, |button, area, highlighted| {
                let button_paragraph = Paragraph::new(button.to_string())
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_set(border::ROUNDED),
                    )
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(
                        if highlighted && self.focus == Focus::PlayButtons {
                            Config::global().theme.highlight
                        } else {
                            Config::global().theme.secondary
                        },
                    ));
                frame.render_widget(button_paragraph, area);
            });

        // the rest of the popup
        // the image
        let image_height = bottom_area.y.saturating_sub(popup_area.y).saturating_sub(3);
        let image_width = (image_height as f32 * RATIO * TERMINAL_RATIO) as u16;
        let image_area = Rect {
            x: popup_area.x + 4,
            y: popup_area.y + 2,
            width: image_width,
            height: image_height,
        };

        ImageManager::render_image(&self.image_manager, anime.as_ref(), frame, image_area, true);

        //title and info area
        let [title_area, info_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Fill(1)])
            .areas(popup_area.inner(Margin::new(1, 1)));
        let title_area_x = image_area.x + image_area.width + 3;
        let title_area = Rect {
            x: title_area_x,
            y: title_area.y,
            width: popup_area.x
                + popup_area
                    .width
                    .saturating_sub(title_area_x)
                    .saturating_sub(2),
            height: title_area.height,
        };
        let info_area = Rect {
            x: title_area.x,
            y: info_area.y,
            width: title_area.width - 1,
            height: info_area.height.saturating_sub(buttons_area.height),
        };

        let title = if anime.alternative_titles.en.is_empty() {
            anime.title.clone()
        } else {
            anime.alternative_titles.en.clone()
        };

        let title_text = Paragraph::new(title)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Config::global().theme.secondary).bold());

        frame.render_widget(title_text, title_area.inner(Margin::new(0, 1)));

        //synopsis
        // FIXME: this needs fixing
        let synopsis_area = Rect {
            x: left.x + 1,
            y: bottom_area.y,
            width: left.width.saturating_sub(1),
            height: bottom_area.height.saturating_sub(1),
        };

        self.synopsis_area = Some(synopsis_area);

        // Calculate the content height for scrollbar
        let content_height = anime.synopsis.lines().count() as u16;
        let visible_height = synopsis_area.height.saturating_sub(2); // Account for borders

        // Create the paragraph widget
        let synopsis_text = Paragraph::new(anime.synopsis.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .title("Synopsis")
                    .style(Style::default().fg(if self.focus == Focus::Synopsis {
                        Config::global().theme.highlight
                    } else {
                        Config::global().theme.primary
                    })),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Config::global().theme.text))
            .scroll((self.synopsis_scroll, 0));

        // Render the paragraph
        frame.render_widget(synopsis_text, synopsis_area);

        // FIXME: above this needs fixing
        // Create and render scrollbar if content is longer than visible area
        if content_height > visible_height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(if self.focus == Focus::Synopsis {
                    Config::global().theme.highlight
                } else {
                    Config::global().theme.primary
                }));

            let mut scrollbar_state = ScrollbarState::new(content_height as usize)
                .position(self.synopsis_scroll as usize);

            frame.render_stateful_widget(
                scrollbar,
                synopsis_area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }), // Position scrollbar inside borders
                &mut scrollbar_state,
            );
        }

        // right side next to image and above buttons
        let [_score, info_area, _buttons] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(0),
                Constraint::Fill(1),
                Constraint::Length(3),
            ])
            .areas(info_area);

        // score text
        let big_text = BigText::builder()
            .style(Style::default().fg(Color::White))
            .pixel_size(PixelSize::Sextant)
            .lines(vec![anime.mean.to_string().into()])
            .build();
        let [_, big_area_vertical] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(5)])
            .areas(info_area);
        let [_, big_text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Length(20)])
            .areas(big_area_vertical);

        // info area
        let [_, info_area_one, _, info_area_two] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(8),
                Constraint::Length(2),
                Constraint::Length(8),
            ])
            .areas(info_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title("Anime Info")
            .style(Style::default().fg(Config::global().theme.primary));

        frame.render_widget(block, info_area);
        frame.render_widget(big_text, big_text_area);

        let startseason = DisplayString::new()
            .add(anime.start_season.to_string())
            .uppercase(0)
            .build("{0}");

        InfoBox::new()
            .add_ranked_item("Ranked", anime.rank.to_string())
            .add_ranked_item("Popularity", anime.popularity.to_string())
            .add_text_item("Members", anime.num_list_users.to_string())
            .add_row()
            .add_text_item("Start", startseason)
            .add_text_item("type", anime.media_type.to_string())
            .add_text_item("studio", anime.studios_as_string())
            .add_row()
            .add_text_item(
                "Episodes",
                format!(
                    "{}/{}",
                    anime.num_released_episodes.unwrap_or(0),
                    if anime.episode_count_ready {
                        anime.num_episodes.to_string()
                    } else {
                        "?".to_string()
                    }
                ),
            )
            .add_text_item("Duration", anime.average_episode_duration.to_string())
            .add_text_item("Rating", anime.rating.to_string())
            .add_row()
            .add_text_item("Status", anime.status.to_string())
            .add_text_item("Source", anime.source.to_string())
            .add_text_item("Id", anime.id.to_string())
            .render(
                frame,
                info_area_one,
                Margin::new(8, 0),
                Config::global().theme.primary,
            );

        InfoBox::new()
            .add_text_item("Added", format_date(&anime.created_at))
            .add_row()
            .add_text_item("Updated", format_date(&anime.updated_at))
            .add_row()
            .add_text_item("Started", format_date(&anime.start_date))
            .add_row()
            .add_text_item("Ended", format_date(&anime.end_date))
            .render(
                frame,
                info_area_two,
                Margin::new(8, 0),
                Config::global().theme.primary,
            );

        // buttons within info area
        let status_buttons_area = Rect {
            x: _buttons.x + (_buttons.width / 10),
            y: _buttons.y,
            width: _buttons.width * 8 / 10,
            height: 3,
        };

        self.status_nav.construct_mut(
            &mut self.status_buttons,
            status_buttons_area,
            |dropdown, area, highlighted| {
                dropdown.render(
                    frame,
                    area,
                    highlighted && self.focus == Focus::StatusButtons,
                );
            },
        );
    }
}

#[derive(Clone)]
pub struct SeasonPopup {
    toggled: bool,
    year_scroll: u16,
    season_scroll: u16,
    year_selected: bool,
    available_years: Vec<String>,
    all_years: Vec<String>,
    entered_number: String,

    //cache
    activate_area: Option<Rect>,
    popup_area: Option<Rect>,
    previous_year: u16,
}
impl SeasonPopup {
    pub fn new() -> Self {
        let (year, season) = MalClient::current_season();
        let season_scroll = AVAILABLE_SEASONS
            .iter()
            .position(|&s| s.to_lowercase() == season.to_lowercase())
            .unwrap_or(0) as u16;

        let all_years: Vec<String> = (FIRST_YEAR..=year).rev().map(|y| y.to_string()).collect();

        Self {
            toggled: false,
            year_scroll: 0,
            season_scroll,
            available_years: all_years.clone(),
            all_years,
            year_selected: false,
            entered_number: String::new(),
            activate_area: None,
            popup_area: None,
            previous_year: year,
        }
    }

    fn filter_years(&mut self) {
        if self.entered_number.is_empty() {
            self.available_years = self.all_years.clone();
        } else {
            self.available_years = self
                .all_years
                .iter()
                .filter(|year| year.contains(&self.entered_number))
                .cloned()
                .collect();
        }
        self.year_scroll = 0;
    }

    pub fn hide(&mut self) -> &Self {
        self.popup_area = None;
        self.toggled = false;
        self.entered_number.clear();
        self.filter_years();
        self
    }

    pub fn toggle(&mut self) -> &Self {
        self.toggled = !self.toggled;

        if self.toggled {
            self.year_scroll = self
                .available_years
                .iter()
                .position(|y| y.parse::<u16>().unwrap_or(0) == self.previous_year)
                .unwrap_or(0) as u16;
        }
        self
    }

    pub fn is_toggled(&self) -> bool {
        self.toggled
    }

    pub fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<(u16, String)> {
        let nav = &Config::global().navigation;

        // for writing (search of numbers)
        match key_event.code {
            KeyCode::Backspace => {
                if !self.entered_number.is_empty() {
                    self.entered_number.pop();
                    self.filter_years();
                }
                return None;
            }
            KeyCode::Char(c) => {
                if c.is_ascii_digit() {
                    self.entered_number.push(c);
                    self.filter_years();
                    return None;
                }
            }
            _ => {}
        }

        // for navigation
        match nav.get_direction(&key_event.code) {
            NavDirection::Right => {
                self.year_selected = false;
                return None;
            }

            NavDirection::Left => {
                self.year_selected = true;
                return None;
            }

            NavDirection::Up => {
                if self.year_selected {
                    self.year_scroll = self.year_scroll.saturating_sub(1);
                } else {
                    self.season_scroll = self.season_scroll.saturating_sub(1);
                }
                return None;
            }
            NavDirection::Down => {
                if self.year_selected {
                    if self.year_scroll < (self.available_years.len().saturating_sub(1)) as u16 {
                        self.year_scroll += 1;
                    }
                } else if self.season_scroll < (AVAILABLE_SEASONS.len().saturating_sub(1)) as u16 {
                    self.season_scroll += 1;
                }
                return None;
            }
            _ => {}
        };

        // for selecting
        if nav.is_select(&key_event.code) {
            if !self.toggled {
                self.toggle();
                return None;
            }

            let (_year, _) = MalClient::current_season();
            let season = AVAILABLE_SEASONS
                .get(self.season_scroll as usize)
                .unwrap_or(&FIRST_SEASON)
                .to_string();

            let year = self
                .available_years
                .get(self.year_scroll as usize)
                .and_then(|y| y.parse::<u16>().ok())
                .unwrap_or(_year);

            self.previous_year = year;
            self.hide();

            return Some((year, season));
        }

        if nav.is_close(&key_event.code) {
            self.hide();
        }

        None
    }

    pub fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<(u16, String)> {
        // if the season area has been rendered yet
        let activate_area = self.activate_area?;
        let mouse_pos = Position::new(mouse_event.column, mouse_event.row);
        if activate_area.contains(mouse_pos) {
            if let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind {
                self.toggle();
            }
        }

        // the popup below the season area
        let popup_area = self.popup_area?;
        let [left, right] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Fill(1)])
            .areas(popup_area);

        // right side
        if right.contains(mouse_pos) {
            self.year_selected = false;
            match mouse_event.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    self.season_scroll = self.season_scroll.saturating_sub(1);
                }

                crossterm::event::MouseEventKind::ScrollDown => {
                    if self.season_scroll < (AVAILABLE_SEASONS.len().saturating_sub(1)) as u16 {
                        self.season_scroll += 1;
                    }
                }
                _ => {}
            }
        }
        // left side
        else if left.contains(mouse_pos) {
            self.year_selected = true;
            match mouse_event.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    self.year_scroll = self.year_scroll.saturating_sub(1);
                }

                crossterm::event::MouseEventKind::ScrollDown => {
                    if self.year_scroll < (self.available_years.len().saturating_sub(1)) as u16 {
                        self.year_scroll += 1;
                    }
                }
                _ => {}
            }
        }

        // now when selecting an option or clicking outside close the popup:
        if matches!(mouse_event.kind, crossterm::event::MouseEventKind::Down(_)) {
            if popup_area.contains(mouse_pos) {
                let (_year, _) = MalClient::current_season();
                let season = AVAILABLE_SEASONS
                    .get(self.season_scroll as usize)
                    .unwrap_or(&FIRST_SEASON)
                    .to_string();

                let year = self
                    .available_years
                    .get(self.year_scroll as usize)
                    .and_then(|y| y.parse::<u16>().ok())
                    .unwrap_or(_year);

                self.previous_year = year;
                self.hide();
                return Some((year, season));
            }

            self.hide();
        }

        None
    }

    pub fn render(&mut self, frame: &mut Frame, season_area: Rect) {
        self.activate_area = Some(season_area);

        if !self.toggled {
            return;
        }

        let area = frame.area();

        let [height, width] = [min(8, area.height), season_area.width * 7 / 20];
        let popup_area = Rect::new(
            season_area.x + (season_area.width.saturating_sub(width)) / 2,
            season_area.y + season_area.height.saturating_sub(1),
            width,
            height,
        );
        self.popup_area = Some(popup_area);
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .style(Style::default().fg(Config::global().theme.primary));
        frame.render_widget(block.clone(), popup_area);

        let text = if self.entered_number.is_empty() {
            self.entered_number.clone()
        } else {
            format!("Search: {}", self.entered_number)
        };
        let paragraph = Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Config::global().theme.primary));
        frame.render_widget(paragraph, popup_area);

        let [year_area, middle_area, season_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .areas(popup_area);
        let season_area = Rect {
            x: season_area.x + 1,
            y: season_area.y + 1,
            width: season_area.width.saturating_sub(2),
            height: season_area.height.saturating_sub(2),
        };
        let year_area = Rect {
            x: year_area.x + 1,
            y: year_area.y + 1,
            width: year_area.width.saturating_sub(2),
            height: year_area.height.saturating_sub(2),
        };

        let divider = "|";
        let left_arrow = if self.year_selected { "◀" } else { " " };
        let right_arrow = if !self.year_selected { "▶" } else { " " };
        let [middle_area_left, middle_area, middle_area_right] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(middle_area);

        let middle_area = Rect {
            x: middle_area.x,
            y: middle_area.y + 2,
            width: middle_area.width,
            height: middle_area.height.saturating_sub(3),
        };

        let middle_area_left = Rect {
            x: middle_area_left.x,
            y: middle_area_left.y + 2,
            width: middle_area_left.width,
            height: middle_area_left.height.saturating_sub(3),
        };

        let middle_area_right = Rect {
            x: middle_area_right.x,
            y: middle_area_right.y + 2,
            width: middle_area_right.width,
            height: middle_area_right.height.saturating_sub(3),
        };

        let left_paragraph = Paragraph::new(left_arrow)
            .block(Block::default().padding(Padding::new(0, 0, middle_area_left.height / 2, 0)))
            .alignment(Alignment::Left)
            .style(Style::default().fg(if self.year_selected {
                Config::global().theme.highlight
            } else {
                Config::global().theme.primary
            }));
        let middle_paragraph = Paragraph::new(divider)
            .block(Block::default().padding(Padding::new(0, 0, middle_area.height / 2, 0)))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Config::global().theme.primary));
        let right_paragraph = Paragraph::new(right_arrow)
            .block(Block::default().padding(Padding::new(0, 0, middle_area_right.height / 2, 0)))
            .alignment(Alignment::Right)
            .style(Style::default().fg(if !self.year_selected {
                Config::global().theme.highlight
            } else {
                Config::global().theme.primary
            }));

        frame.render_widget(left_paragraph, middle_area_left);
        frame.render_widget(middle_paragraph, middle_area);
        frame.render_widget(right_paragraph, middle_area_right);

        for (i, season) in AVAILABLE_SEASONS.iter().enumerate() {
            let y_position = (3 + season_area.y + i as u16).saturating_sub(self.season_scroll);
            if y_position >= season_area.y + season_area.height {
                break;
            }
            let individual_season_area = Rect {
                x: season_area.x,
                y: y_position,
                width: season_area.width,
                height: 1,
            };
            let paragraph = Paragraph::new(season.to_string())
                .alignment(Alignment::Center)
                .style(Style::default().fg(
                    if !self.year_selected && self.season_scroll == i as u16 {
                        Config::global().theme.highlight
                    } else {
                        Config::global().theme.primary
                    },
                ));
            frame.render_widget(paragraph, individual_season_area);
        }

        for (i, year) in self.available_years.iter().enumerate() {
            let y_position = (3 + year_area.y + i as u16).saturating_sub(self.year_scroll);
            if y_position >= year_area.y + year_area.height {
                break;
            } else if y_position < year_area.y {
                continue;
            }
            let individual_year_area = Rect {
                x: year_area.x,
                y: y_position,
                width: year_area.width,
                height: 1,
            };
            let paragraph = Paragraph::new(year.to_string())
                .alignment(Alignment::Center)
                .style(Style::default().fg(
                    if self.year_selected && self.year_scroll == i as u16 {
                        Config::global().theme.highlight
                    } else {
                        Config::global().theme.primary
                    },
                ));
            frame.render_widget(paragraph, individual_year_area);
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Arrows {
    None,
    Static,
    Dynamic,
}

#[derive(Clone)]
pub struct SelectionPopup {
    is_open: bool,
    options: Vec<String>,
    selected_index: usize,
    next_index: usize,
    arrows: Arrows,
    longest_word: usize,
    displaying_format: String,
    color: Color,
    area: Option<Rect>,
    popup_area: Option<Rect>,
    scroll: usize,
}

impl SelectionPopup {
    pub fn new() -> Self {
        Self {
            is_open: false,
            options: Vec::new(),
            selected_index: 0,
            next_index: 0,
            arrows: Arrows::None,
            longest_word: 0,
            displaying_format: String::new(),
            color: Config::global().theme.primary,
            area: None,
            popup_area: None,
            scroll: 0,
        }
    }

    pub fn get_selected_option(&self) -> Option<String> {
        if self.options.is_empty() {
            None
        } else {
            Some(self.options[self.selected_index].clone())
        }
    }

    pub fn with_arrows(mut self, arrow_type: Arrows) -> Self {
        self.arrows = arrow_type;
        self
    }

    pub fn with_selected_option(mut self, option: String) -> Self {
        if let Some(index) = self
            .options
            .iter()
            .position(|o| o.to_lowercase() == option.to_lowercase())
        {
            self.selected_index = index;
            self.next_index = index;
        } else {
            self.selected_index = 0;
            self.next_index = 0;
        }
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    pub fn add_option(mut self, option: impl Into<String>) -> Self {
        let option = option.into();
        if option.len() > self.longest_word {
            self.longest_word = option.len();
        }
        self.options.push(option);
        self
    }

    pub fn add_options<I, S>(mut self, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for option in options {
            self = self.add_option(option);
        }
        self
    }

    pub fn with_displaying_format<T: Into<String>>(mut self, text: T) -> Self {
        self.displaying_format = text.into();
        self
    }

    pub fn toggle(&mut self) -> &Self {
        self.is_open = !self.is_open;
        self
    }
    pub fn open(&mut self) -> &Self {
        self.is_open = true;
        self
    }

    pub fn close(&mut self) -> &Self {
        self.is_open = false;
        self
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn is_hovered(&self, mouse_event: MouseEvent) -> bool {
        if let Some(box_area) = self.area {
            let pos = Position::new(mouse_event.column, mouse_event.row);
            if box_area.contains(pos) {
                return true;
            }
        }

        false
    }

    pub fn handle_input(&mut self, key_event: KeyEvent) -> Option<String> {
        let nav = &Config::global().navigation;

        if !self.is_open {
            if key_event.code == KeyCode::Enter {
                self.open();
            }
            return None;
        }

        match nav.get_direction(&key_event.code) {
            NavDirection::Up => {
                self.next_index = self.next_index.saturating_sub(1);
                return None;
            }
            NavDirection::Down => {
                if self.next_index < self.options.len().saturating_sub(1) {
                    self.next_index += 1;
                }
                return None;
            }
            _ => {}
        }

        if nav.is_select(&key_event.code) {
            if self.options.is_empty() {
                return None;
            }

            let selected_option = self.options[self.next_index].clone();
            self.selected_index = self.next_index;
            self.close();
            return Some(selected_option);
        }

        if nav.is_close(&key_event.code) {
            self.close();
        }

        None
    }

    pub fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<String> {
        let mouse_clicked = matches!(mouse_event.kind, crossterm::event::MouseEventKind::Down(_));
        let pos = Position::new(mouse_event.column, mouse_event.row);

        // this is the button area itself
        let box_area = self.area?;
        if box_area.contains(pos) && mouse_clicked {
            self.toggle();
            return None;
        }

        // this handles inside the popup box
        let popup_area = self.popup_area?;

        // for scrolling:
        if popup_area.contains(pos) {
            match mouse_event.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    self.next_index = self.next_index.saturating_sub(1);
                    return None;
                }

                crossterm::event::MouseEventKind::ScrollDown => {
                    if self.next_index < (self.options.len().saturating_sub(1)) {
                        self.next_index += 1;
                    }
                    return None;
                }
                _ => {}
            }
        }

        // for clicking and highlighting
        for (i, row) in popup_area.inner(Margin::new(0, 1)).rows().enumerate() {
            if row.contains(pos) && i <= self.options.len() {
                self.next_index = i + self.scroll;
                if mouse_clicked {
                    let selected_option = self.options[self.next_index].clone();
                    self.selected_index = self.next_index;
                    self.close();
                    return Some(selected_option);
                }
            }
        }

        // this happens only whenever the mouse isn't clicked on the button or inside the popup
        if mouse_clicked {
            self.close();
        }

        None
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, highlighted: bool) {
        self.area = Some(area);
        let option = self
            .options
            .get(self.selected_index)
            .unwrap_or(&"No options available".to_string())
            .clone();
        let option = if self.displaying_format.is_empty() {
            option
        } else {
            self.displaying_format.replace("{}", &option)
        };

        let filter = Paragraph::new(option)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(if highlighted {
                Config::global().theme.highlight
            } else {
                self.color
            }));
        frame.render_widget(filter, area);

        if self.is_open {
            let terminal_height = frame.area().height;
            let available_space_below = terminal_height.saturating_sub(area.y + area.height);
            let needed_height = self.options.len() as u16 + 2;
            let popup_height = std::cmp::min(needed_height, available_space_below);
            if popup_height < 3 {
                return;
            }

            let options_area = Rect::new(area.x, area.y + area.height, area.width, popup_height);
            frame.render_widget(Clear, options_area);
            self.popup_area = Some(options_area);

            let options_block = Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .style(Style::default().fg(Config::global().theme.primary));
            frame.render_widget(options_block, options_area);

            let max_visible_options = (popup_height.saturating_sub(2)) as usize;

            // Auto-scroll to keep next_index visible
            if self.next_index < self.scroll {
                self.scroll = self.next_index;
            } else if self.next_index >= self.scroll + max_visible_options {
                self.scroll = self.next_index + 1 - max_visible_options;
            }

            let visible_options = self
                .options
                .iter()
                .enumerate()
                .skip(self.scroll)
                .take(max_visible_options);

            for (display_row, (original_index, option)) in visible_options.enumerate() {
                let option_area = Rect::new(
                    options_area.x + 1,
                    options_area.y + display_row as u16 + 1,
                    options_area.width.saturating_sub(2),
                    1,
                );

                let [left_side, option_area, right_side] = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Fill(1),
                        Constraint::Length(std::cmp::min(
                            self.longest_word as u16 + 2,
                            option_area.width.saturating_sub(2),
                        )),
                        Constraint::Fill(1),
                    ])
                    .areas(option_area);

                if original_index == self.next_index {
                    let mut text = option.to_string();

                    if self.arrows == Arrows::Dynamic {
                        text = format!("▶ {} ◀", option);
                    }

                    let option_paragraph = Paragraph::new(text)
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Config::global().theme.highlight));
                    frame.render_widget(option_paragraph, option_area);

                    if self.arrows != Arrows::Static {
                        continue;
                    }

                    let left_paragraph = Paragraph::new("▶")
                        .alignment(Alignment::Right)
                        .style(Style::default().fg(Config::global().theme.highlight));

                    let right_paragraph = Paragraph::new("◀")
                        .alignment(Alignment::Left)
                        .style(Style::default().fg(Config::global().theme.highlight));

                    frame.render_widget(left_paragraph, left_side);
                    frame.render_widget(right_paragraph, right_side);
                } else {
                    let option_paragraph = Paragraph::new(option.to_string())
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Config::global().theme.primary));
                    frame.render_widget(option_paragraph, option_area);
                }
            }

            if self.options.len() > max_visible_options {
                let scroll_info_area = Rect::new(
                    options_area.x + options_area.width.saturating_sub(1),
                    options_area.y + 1,
                    1,
                    options_area.height.saturating_sub(2),
                );

                if self.scroll > 0 {
                    frame.render_widget(
                        Paragraph::new("↑")
                            .style(Style::default().fg(Config::global().theme.highlight)),
                        Rect::new(scroll_info_area.x, scroll_info_area.y, 1, 1),
                    );
                }

                if self.scroll + max_visible_options < self.options.len() {
                    frame.render_widget(
                        Paragraph::new("↓")
                            .style(Style::default().fg(Config::global().theme.highlight)),
                        Rect::new(
                            scroll_info_area.x,
                            scroll_info_area.y + scroll_info_area.height.saturating_sub(1),
                            1,
                            1,
                        ),
                    );
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ErrorPopup {
    toggled: bool,
    error_message: String,
    height: u16,
    width: u16,
}

impl ErrorPopup {
    pub fn new() -> Self {
        Self {
            toggled: false,
            error_message: String::new(),
            height: 10,
            width: 40,
        }
    }

    pub fn is_open(&self) -> bool {
        self.toggled
    }

    pub fn set_error(&mut self, message: String) -> &Self {
        let content_width = self.width.saturating_sub(2);
        let total_lines: u16 = message
            .lines()
            .map(|line| {
                if line.is_empty() {
                    1
                } else {
                    (line.len() as u16).div_ceil(content_width)
                }
            })
            .sum();

        self.height = total_lines + 2;
        self.error_message = message;
        self
    }

    pub fn open(&mut self) -> &Self {
        self.toggled = true;
        self
    }

    pub fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        if !self.toggled {
            return None;
        }
        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.toggled = false;
                self.error_message.clear();
                None
            }
            _ => None,
        }
    }

    pub fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<Action> {
        if !self.toggled {
            return None;
        }
        match mouse_event.kind {
            MouseEventKind::Down(_) => {
                self.toggled = false;
                self.error_message.clear();
                None
            }
            _ => None,
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        if !self.toggled {
            return;
        }

        let area = frame.area();

        let max_width = std::cmp::min(
            self.width,
            std::cmp::max(area.width * 4 / 5, area.width.saturating_sub(4)),
        );
        let max_height = std::cmp::min(
            self.height,
            std::cmp::max(area.height * 4 / 5, area.height.saturating_sub(4)),
        );

        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(max_width)) / 2,
            area.y + (area.height.saturating_sub(max_height)) / 2,
            max_width,
            max_height,
        );

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title("Error")
            .style(Style::default().fg(Config::global().theme.error));

        frame.render_widget(block.clone(), popup_area);

        let paragraph = Paragraph::new(self.error_message.clone())
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .scroll((0, 0))
            .style(Style::default().fg(Config::global().theme.error));

        frame.render_widget(paragraph, popup_area);
    }
}

// #[derive(Clone)]
// pub struct SearchPopup {
//     pub toggled: bool,
//     pub query: String,
// }
