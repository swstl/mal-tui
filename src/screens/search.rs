use super::ExtraInfo;
use super::widgets::animebox::AnimeBox;
use super::widgets::navigatable::Navigatable;
use super::widgets::popup::{Arrows, SelectionPopup};
use crate::add_screen_caching;
use crate::app::Event;
use crate::config::Config;
use crate::config::navigation::NavDirection;
use crate::mal::models::anime::Anime;
use crate::mal::models::anime::AnimeId;
use crate::utils::functionStreaming::StreamableRunner;
use crate::utils::imageManager::ImageManager;
use crate::utils::input::Input;
use crate::{app::Action, screens::Screen};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::{Alignment, Position};
use ratatui::style;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::symbols::border;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;
use std::thread::JoinHandle;

#[derive(Debug, Clone)]
enum LocalEvent {
    FilterSwitch(String),
    Search(String),
}

#[derive(PartialEq, Debug, Clone)]
enum Focus {
    NavBar,
    Filter,
    Search,
    AnimeList,
}

#[derive(Clone)]
pub struct SearchScreen {
    animes: Vec<AnimeId>,
    image_manager: Arc<Mutex<ImageManager>>,
    app_info: ExtraInfo,

    navigatable: Navigatable,
    focus: Focus,

    filter_popup: SelectionPopup,
    search_input: Input,
    search_area: Option<ratatui::layout::Rect>,

    fetching: bool,
    bg_sender: Option<Sender<LocalEvent>>,
    bg_loaded: bool,
}

impl SearchScreen {
    pub fn new(info: ExtraInfo) -> Self {
        Self {
            image_manager: Arc::new(Mutex::new(ImageManager::new())),
            navigatable: Navigatable::new((3, 2)),
            filter_popup: SelectionPopup::new()
                .with_arrows(Arrows::Static)
                .add_option("all")
                .add_option("airing")
                .add_option("upcoming")
                .add_option("tv")
                .add_option("ova")
                .add_option("movie")
                .add_option("special")
                .add_option("popularity")
                .add_option("favorite"),
            search_input: Input::new(),
            focus: Focus::NavBar,
            animes: Vec::new(),
            search_area: None,
            bg_loaded: false,
            bg_sender: None,
            fetching: false,
            app_info: info,
        }
    }

    fn reset(&mut self) {
        self.navigatable.back_to_start();
        self.animes.clear();
        self.fetching = false;
    }

    fn int_length(&self, mut n: usize) -> usize {
        if n == 0 {
            return 1;
        }
        let mut len = 0;
        while n > 0 {
            n /= 10;
            len += 1;
        }
        len
    }

    fn fetch_and_send_animes<F>(app_sx: &Sender<Event>, id: String, fetch_fn: F)
    where
        F: FnMut(usize, usize) -> Option<Vec<Anime>>,
    {
        let anime_generator = StreamableRunner::new()
            .change_batch_size_at(100, 1)
            .stop_at(2);

        for animes in anime_generator.run(fetch_fn) {
            let anime_ids = animes
                .iter()
                .map(|anime| anime.id)
                .collect::<Vec<AnimeId>>();
            let update = super::BackgroundUpdate::new(id.clone())
                .set("animes", animes)
                .set("anime_ids", anime_ids);
            app_sx.send(super::Event::BackgroundNotice(update)).ok();
        }
    }
}

impl Screen for SearchScreen {
    add_screen_caching!();

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);

        let [_top, bottom] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Percentage(100)])
            .areas(area);

        let [result_area, bottom_middle, _] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .areas(bottom);

        if !self.animes.is_empty() {
            let width = self.int_length(self.animes.len()) as u16 + 4;

            let [_, result_area, _] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(width + 4),
                    Constraint::Fill(1),
                ])
                .areas(result_area);

            let [result_area, _] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Fill(1)])
                .areas(result_area);

            let results = Paragraph::new(self.animes.len().to_string())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_set(symbols::border::ROUNDED),
                )
                .style(Style::default().fg(Config::global().theme.primary));
            frame.render_widget(results, result_area);
        }

        let [search_area, _, anime_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(bottom_middle);

        let [search_area, filter_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
            .areas(search_area);

        let search_field = Paragraph::new(self.search_input.value())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Search")
                    .border_set(border::ROUNDED),
            )
            .style(style::Style::default().fg(if self.focus == Focus::Search {
                Config::global().theme.highlight
            } else {
                Config::global().theme.primary
            }));
        frame.render_widget(search_field, search_area);

        self.navigatable
            .construct(&self.animes, anime_area, |anime_id, area, highlight| {
                if let Some(anime) = self.app_info.anime_store.get(anime_id) {
                    AnimeBox::render(
                        &anime,
                        &self.image_manager,
                        frame,
                        area,
                        highlight && self.focus == Focus::AnimeList,
                    );
                }
            });
        self.search_area = Some(search_area);
        self.search_input.render_cursor(
            frame,
            search_area.x + 1,
            search_area.y + 1,
            self.focus == Focus::Search,
        );
        self.filter_popup
            .render(frame, filter_area, self.focus == Focus::Filter);
    }

    fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        let modifier = key_event
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        let nav = &Config::global().navigation;

        match self.focus {
            Focus::Filter => {
                if modifier {
                    match nav.get_direction(&key_event.code) {
                        NavDirection::Down => {
                            self.focus = Focus::AnimeList;
                            self.filter_popup.close();
                        }
                        NavDirection::Up => {
                            self.focus = Focus::NavBar;
                            self.filter_popup.close();
                            return Some(Action::NavbarSelect(true));
                        }
                        NavDirection::Left => {
                            self.focus = Focus::Search;
                            self.filter_popup.close();
                        }
                        _ => {}
                    }
                    return None;
                }

                if let Some(mut filter_type) = self.filter_popup.handle_input(key_event) {
                    self.fetching = true;
                    if filter_type == "popularity" {
                        filter_type = "bypopularity".to_string();
                    }
                    if let Some(sender) = &self.bg_sender {
                        sender.send(LocalEvent::FilterSwitch(filter_type)).ok();
                    }
                }
            }

            Focus::Search => {
                if modifier {
                    match nav.get_direction(&key_event.code) {
                        NavDirection::Up => {
                            self.focus = Focus::NavBar;
                            return Some(Action::NavbarSelect(true));
                        }
                        NavDirection::Down => {
                            self.focus = Focus::AnimeList;
                            return None;
                        }
                        NavDirection::Right => {
                            self.focus = Focus::Filter;
                            return None;
                        }
                        _ => {}
                    }
                }

                if let Some(text) = self.search_input.handle_event(key_event, false)
                    && !text.is_empty()
                {
                    self.fetching = true;
                    if let Some(sender) = &self.bg_sender {
                        sender.send(LocalEvent::Search(text)).ok();
                    }
                }
            }

            Focus::AnimeList => {
                if modifier {
                    if nav.get_direction(&key_event.code) == NavDirection::Up {
                        self.focus = Focus::Search;
                    }
                    return None;
                }

                match nav.get_direction(&key_event.code) {
                    NavDirection::Down => {
                        self.navigatable.move_down();
                    }
                    NavDirection::Up => {
                        self.navigatable.move_up();
                    }
                    NavDirection::Right => {
                        self.navigatable.move_right();
                    }
                    NavDirection::Left => {
                        self.navigatable.move_left();
                    }
                    _ => {}
                }

                if nav.is_select(&key_event.code)
                    && let Some(anime_id) = self.navigatable.get_selected_item(&self.animes)
                    && let Some(anime) = self.app_info.anime_store.get(anime_id)
                {
                    return Some(Action::ShowOverlay(anime.id));
                }
            }

            Focus::NavBar => {
                self.focus = Focus::Search;
            }
        }

        None
    }

    fn handle_mouse(&mut self, mouse_event: crossterm::event::MouseEvent) -> Option<Action> {
        if mouse_event.row < 3 {
            self.focus = Focus::NavBar;
            return Some(Action::NavbarSelect(true));
        }

        if self.filter_popup.is_hovered(mouse_event) || self.filter_popup.is_open() {
            self.focus = Focus::Filter;

            // if a filter is selected:
            let mut filter = self.filter_popup.handle_mouse(mouse_event)?;

            self.fetching = true;
            if filter == "popularity" {
                filter = "bypopularity".to_string();
            }

            let sender = self.bg_sender.clone()?;
            sender.send(LocalEvent::FilterSwitch(filter)).ok();

            return None;
        }

        if let Some(search_area) = self.search_area {
            let pos = Position::new(mouse_event.column, mouse_event.row);
            if search_area.contains(pos) {
                self.focus = Focus::Search;
                return None;
            }
        }

        if self.navigatable.is_hovered(mouse_event) {
            self.focus = Focus::AnimeList;
            self.navigatable.handle_scroll(mouse_event);
        }

        if self.navigatable.get_hovered_index(mouse_event).is_some()
            && let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind
        {
            let anime_id = self.navigatable.get_selected_item(&self.animes)?;
            return Some(Action::ShowOverlay(*anime_id));
        }

        None
    }

    fn background(&mut self) -> Option<JoinHandle<()>> {
        if self.bg_loaded {
            return None;
        }

        let info = self.app_info.clone();
        let nr_of_animes = self.animes.len();
        self.bg_loaded = true;
        let (bg_sender, bg_receiver) = channel::<LocalEvent>();
        self.bg_sender = Some(bg_sender);
        let id = self.get_name();
        let image_manager = self.image_manager.clone();
        ImageManager::init_with_threads(&image_manager, info.app_sx.clone());
        let mal_client = info.mal_client.clone();
        let app_sx = info.app_sx.clone();

        let handle = std::thread::spawn(move || {
            if nr_of_animes == 0 {
                Self::fetch_and_send_animes(&app_sx, id.clone(), |offset, limit| {
                    mal_client.get_top_anime("all".to_string(), offset, limit)
                });
            }

            while let Ok(event) = bg_receiver.recv() {
                match event {
                    LocalEvent::FilterSwitch(filter_type) => {
                        Self::fetch_and_send_animes(&app_sx, id.clone(), |offset, limit| {
                            mal_client.get_top_anime(filter_type.clone(), offset, limit)
                        });
                    }

                    LocalEvent::Search(query) => {
                        Self::fetch_and_send_animes(&app_sx, id.clone(), |offset, limit| {
                            info.mal_client.search_anime(query.clone(), offset, limit)
                        });
                    }
                }
            }
        });
        Some(handle)
    }

    fn apply_update(&mut self, mut update: super::BackgroundUpdate) {
        if let Some(ids) = update.take::<Vec<AnimeId>>("anime_ids") {
            if self.fetching {
                self.reset();
            }
            self.animes.extend(ids);
        }
    }
}
