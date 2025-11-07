use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::{add_screen_caching, check_for_account};
use crate::app::Event;
use crate::config::navigation::NavDirection;
use crate::config::Config;
use crate::mal::models::anime::{Anime, AnimeId};
use crate::utils::functionStreaming::StreamableRunner;
use crate::utils::imageManager::ImageManager;
use crate::utils::input::Input;
use crate::{app::Action, screens::Screen};

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Layout;
use ratatui::layout::{Alignment, Constraint, Margin, Rect};
use ratatui::layout::{Direction, Position};
use ratatui::style;
use ratatui::symbols::border;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;

use super::widgets::animebox::AnimeBox;
use super::widgets::navigatable::Navigatable;
use super::widgets::popup::{Arrows, SelectionPopup};
use super::{BackgroundUpdate, ExtraInfo};

#[derive(Debug, Clone)]
struct Statistics {
    pub total_animes: usize,
    pub animes_in_list: usize,
    pub filteres_animes: usize,
}

impl Statistics {
    fn new() -> Self {
        Self {
            total_animes: 0,
            animes_in_list: 0,
            filteres_animes: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Filters {
    list_type: String,
    airing_type: String,
    anime_type: String,
    sort_by: String,
    sort_order: String,
}

impl Filters {
    fn new() -> Self {
        Self {
            list_type: "all".to_string(),
            airing_type: "all".to_string(),
            anime_type: "all".to_string(),
            sort_by: "by last updated".to_string(),
            sort_order: "ascending".to_string(),
        }
    }

    fn update(&mut self, index: usize, value: String) {
        match index {
            0 => self.list_type = value,
            1 => self.airing_type = value,
            2 => self.anime_type = value,
            3 => self.sort_by = value,
            4 => self.sort_order = value,
            _ => {}
        }
    }
}

enum LocalEvent {
    Dropdown(Vec<Anime>, Filters),
    Search(Vec<Anime>, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    NavBar,
    Content,
    Search,
    Dropdown,
}

#[derive(Clone)]
pub struct ListScreen {
    all_animes: Vec<AnimeId>,
    filtered_animes: Vec<AnimeId>,
    filters: Filters,
    statistics: Statistics,

    bg_loaded: bool,
    bg_sx: Option<Sender<LocalEvent>>,
    bg_startup: bool,
    bg_fetching: bool,
    image_manager: Arc<Mutex<ImageManager>>,

    focus: Focus,
    app_info: ExtraInfo,

    search_input: Input,
    search_area: Option<ratatui::layout::Rect>,

    navigatable: Navigatable,
    dropdowns: Vec<SelectionPopup>,
    dropdown_nav: Navigatable,
}

impl ListScreen {
    pub fn new(info: ExtraInfo) -> Self {
        Self {
            image_manager: Arc::new(Mutex::new(ImageManager::new())),
            navigatable: Navigatable::new((3, 3)),
            dropdown_nav: Navigatable::new((5, 1)),
            dropdowns: vec![
                SelectionPopup::new()
                    .with_arrows(Arrows::Static)
                    .add_option("all")
                    .add_option("watching")
                    .add_option("plan to watch")
                    .add_option("completed")
                    .add_option("on hold")
                    .add_option("dropped"),
                SelectionPopup::new()
                    .with_arrows(Arrows::Static)
                    .add_option("all")
                    .add_option("airing")
                    .add_option("upcoming")
                    .add_option("finished"),
                SelectionPopup::new()
                    .with_arrows(Arrows::Static)
                    .add_option("all")
                    .add_option("tv")
                    .add_option("movie")
                    .add_option("ova")
                    .add_option("ona")
                    .add_option("special"),
                SelectionPopup::new()
                    .with_arrows(Arrows::Static)
                    .add_option("sort")
                    .add_option("by title")
                    .add_option("by score")
                    .add_option("by last updated")
                    .add_option("by episodes")
                    .add_option("by popularity")
                    .add_option("by start date")
                    .add_option("by end date"),
                SelectionPopup::new()
                    .with_arrows(Arrows::Static)
                    .add_option("ascending")
                    .add_option("descending"),
            ],
            statistics: Statistics::new(),
            search_input: Input::new(),
            filters: Filters::new(),
            filtered_animes: Vec::new(),
            all_animes: Vec::new(),
            focus: Focus::NavBar,
            search_area: None,
            bg_fetching: true,
            bg_startup: true,
            bg_loaded: false,
            app_info: info,
            bg_sx: None,
        }
    }

    fn sort_animes(animes: &mut [Anime], sort_by: &str, order: &str) {
        match sort_by {
            "by title" => {
                animes.sort_by(|a, b| a.title.cmp(&b.title));
            }
            "by episodes" => {
                animes.sort_by(|a, b| a.num_episodes.cmp(&b.num_episodes));
            }
            "by popularity" => {
                animes.sort_by(|a, b| a.popularity.cmp(&b.popularity));
            }
            "by start date" => {
                animes.sort_by(|a, b| a.start_date.cmp(&b.start_date));
            }
            "by end date" => {
                animes.sort_by(|a, b| a.end_date.cmp(&b.end_date));
            }
            "by score" => {
                animes.sort_by(|a, b| {
                    a.mean
                        .partial_cmp(&b.mean)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            _ => {}
        }

        if order == "descending" {
            animes.reverse();
        }
    }

    fn filter_animes(animes: &mut Vec<Anime>, filters: &Filters) {
        if filters.list_type != "all" {
            animes.retain(|anime| anime.my_list_status.status == filters.list_type);
        }

        if filters.airing_type != "all" {
            animes.retain(|anime| anime.status == filters.airing_type);
        }

        if filters.anime_type != "all" {
            animes.retain(|anime| anime.media_type == filters.anime_type);
        }

        Self::sort_animes(animes, &filters.sort_by, &filters.sort_order);
    }

    fn search_animes(animes: &mut Vec<Anime>, search: String) {
        if search.is_empty() {
            return;
        }

        let search_lower = search.to_lowercase();
        animes.retain(|anime| {
            anime.title.to_lowercase().contains(&search_lower)
                || anime
                    .alternative_titles
                    .en
                    .to_lowercase()
                    .contains(&search_lower)
                || anime
                    .alternative_titles
                    .ja
                    .to_lowercase()
                    .contains(&search_lower)
                || anime
                    .alternative_titles
                    .synonyms
                    .iter()
                    .any(|syn| syn.to_lowercase().contains(&search_lower))
        });
    }
}

impl Screen for ListScreen {
    add_screen_caching!();
    check_for_account!();

    // draws the screen
    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);

        let [top, bottom] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Percentage(100)])
            .areas(area);

        let [side, bottom, _] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Fill(1),
                Constraint::Percentage(20),
            ])
            .areas(bottom);

        let [search, _, content] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(bottom);

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
        frame.render_widget(search_field, search);

        let info_area = Rect::new(
            side.x + side.width.div_ceil(2) - ((side.width + 1) * 4 / 10),
            content.y,
            (side.width + 1) * 8 / 10,
            content.height * 3 / 10,
        );

        let dropdown_area = Rect::new(
            top.x + top.width - side.width.div_ceil(2) - info_area.width / 2,
            info_area.y,
            info_area.width,
            info_area.height.max(self.dropdowns.len() as u16 * 3),
        );

        let [info_area_left, info_area_right] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .areas(info_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .style(style::Style::default().fg(Config::global().theme.primary));
        frame.render_widget(block, info_area);

        let info = Paragraph::new(" Animes found:\n Selected list:\n")
            .block(Block::default().borders(Borders::TOP).title("Info"))
            .alignment(Alignment::Left)
            .style(style::Style::default().fg(Config::global().theme.primary));

        let info_value = Paragraph::new(format!(
            "{}/{}\n0\n",
            self.filtered_animes.len(),
            self.all_animes.len()
        ))
        .alignment(Alignment::Left)
        .style(style::Style::default().fg(Config::global().theme.primary));
        frame.render_widget(info, info_area_left.inner(Margin::new(1, 0)));
        frame.render_widget(info_value, info_area_right.inner(Margin::new(1, 1)));

        if self.bg_fetching {
            let [_, content, _] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .areas(content);
            let loading_text = Paragraph::new("Loading...")
                .alignment(Alignment::Center)
                .style(style::Style::default().fg(Config::global().theme.primary));
            frame.render_widget(loading_text, content);
        } else {
            let items = self.navigatable.get_visible_items(&self.filtered_animes);
            let animes = self.app_info.anime_store.get_bulk(items);

            self.navigatable.construct(
                &self.filtered_animes,
                content,
                |anime_id, area, highlight| {
                    let anime = animes.iter().find(|a| a.id == *anime_id);
                    if let Some(anime) = anime {
                        AnimeBox::render(
                            anime,
                            &self.image_manager,
                            frame,
                            area,
                            highlight && self.focus == Focus::Content,
                        );
                    }
                },
            );
        }
        self.dropdown_nav.as_reverse().construct_mut(
            &mut self.dropdowns,
            dropdown_area,
            |dropdown, area, highlight| {
                dropdown.render(frame, area, highlight && self.focus == Focus::Dropdown);
            },
        );

        self.search_area = Some(search);
        self.search_input.render_cursor(
            frame,
            search.x + 1,
            search.y + 1,
            self.focus == Focus::Search,
        );
    }

    fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        let modifier = key_event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL);
        let nav = &Config::global().navigation;

        match self.focus {
            Focus::Search => {
                if modifier {
                    match nav.get_direction(&key_event.code) {
                        NavDirection::Up => {
                            self.focus = Focus::NavBar;
                            return Some(Action::NavbarSelect(true));
                        }
                        NavDirection::Down => {
                            self.focus = Focus::Content;
                            return None;
                        }
                        NavDirection::Right => {
                            self.focus = Focus::Dropdown;
                            return None;
                        }
                        _ => {}
                    }
                }

                if let Some(text) = self.search_input.handle_event(key_event, true) {
                    if let Some(sx) = &self.bg_sx {
                        let animes = self.app_info.anime_store.get_bulk(self.all_animes.clone());
                        sx.send(LocalEvent::Search(
                            animes.iter().map(|rc| (**rc).clone()).collect(),
                            text,
                        ))
                        .ok();
                    }
                }
            }

            Focus::NavBar => {
                self.focus = Focus::Search;
            }

            Focus::Content => {
                if modifier {
                    match nav.get_direction(&key_event.code) {
                        NavDirection::Up => {
                            self.focus = Focus::Search;
                            return None;
                        }
                        NavDirection::Right => {
                            self.focus = Focus::Dropdown;
                            return None;
                        }
                        _ => {}
                    }
                    return None;
                }

                match nav.get_direction(&key_event.code) {
                    NavDirection::Up => {
                        self.navigatable.move_up();
                    }
                    NavDirection::Down => {
                        self.navigatable.move_down();
                    }
                    NavDirection::Right => {
                        self.navigatable.move_right();
                    }
                    NavDirection::Left => {
                        self.navigatable.move_left();
                    }
                    _ => {}
                }

                if nav.is_select(&key_event.code) {
                    if let Some(anime_id) =
                        self.navigatable.get_selected_item(&self.filtered_animes)
                    {
                        return Some(Action::ShowOverlay(*anime_id));
                    }
                }
            }

            Focus::Dropdown => {
                if modifier {
                    match nav.get_direction(&key_event.code) {
                        NavDirection::Down | NavDirection::Left => {
                            self.focus = Focus::Content;
                            if let Some(dropdown) =
                                self.dropdown_nav.get_selected_item_mut(&mut self.dropdowns)
                            {
                                dropdown.close();
                            }
                        }
                        NavDirection::Up => {
                            self.focus = Focus::Search;
                            if let Some(dropdown) =
                                self.dropdown_nav.get_selected_item_mut(&mut self.dropdowns)
                            {
                                dropdown.close();
                            }
                            return None;
                        }
                        _ => {}
                    }
                    return None;
                }

                if let Some(dropdown) = self.dropdown_nav.get_selected_item_mut(&mut self.dropdowns)
                {
                    if !dropdown.is_open() {
                        match nav.get_direction(&key_event.code) {
                            NavDirection::Up => {
                                self.dropdown_nav.move_up();
                                return None;
                            }
                            NavDirection::Down => {
                                self.dropdown_nav.move_down();
                                return None;
                            }
                            _ => {}
                        }
                    }

                    if let Some(selection) = dropdown.handle_input(key_event) {
                        let index = self.dropdown_nav.get_selected_index();
                        self.filters.update(index, selection);

                        let animes = self.app_info.anime_store.get_bulk(self.all_animes.clone());

                        if let Some(sx) = &self.bg_sx {
                            sx.send(LocalEvent::Dropdown(
                                animes.iter().map(|rc| (**rc).clone()).collect(),
                                self.filters.clone(),
                            ))
                            .ok();
                        }
                    }
                }
            }
        }
        None
    }

    fn handle_mouse(&mut self, mouse_event: crossterm::event::MouseEvent) -> Option<Action> {
        if mouse_event.row < 3 {
            self.focus = Focus::NavBar;
            return Some(Action::NavbarSelect(true));
        }

        // the dropdowns right side
        // if a dropdown is open it takes priority
        // otherwise check if hovering over any dropdown
        let dropdown = match self
            .dropdown_nav
            .get_selected_item_mut(&mut self.dropdowns){
            Some(d) if d.is_open() => Some(d),
            _ => self.dropdown_nav.get_hovered_item_mut(&mut self.dropdowns, mouse_event)
        };

        if let Some(dropdown) = dropdown {
            self.focus = Focus::Dropdown;

            let selection = dropdown.handle_mouse(mouse_event)?;
            let index = self.dropdown_nav.get_selected_index();
            self.filters.update(index, selection);

            let animes = self.app_info.anime_store.get_bulk(self.all_animes.clone());
            if let Some(sx) = &self.bg_sx {
                sx.send(LocalEvent::Dropdown(
                    animes.iter().map(|rc| (**rc).clone()).collect(),
                    self.filters.clone(),
                ))
                .ok();
            }

            return None;
        }

        // the search box
        if let Some(search_area) = self.search_area {
            let pos = Position::new(mouse_event.column, mouse_event.row);
            if search_area.contains(pos) {
                self.focus = Focus::Search;
                return None;
            }
        }


        // the animes list
        if self.navigatable.is_hovered(mouse_event) {
            self.focus = Focus::Content;
            self.navigatable.handle_scroll(mouse_event);
        }

        if self.navigatable.get_hovered_index(mouse_event).is_some() {
            if let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind {
                let anime_id = self.navigatable.get_selected_item(&self.filtered_animes)?;
                return Some(Action::ShowOverlay(*anime_id));
            }
        }

        None
    }

    fn background(&mut self) -> Option<JoinHandle<()>> {
        if self.bg_loaded {
            return None;
        }
        self.bg_loaded = true;

        let info = self.app_info.clone();
        let id = self.get_name();
        let (sx, rx) = channel::<LocalEvent>();
        self.bg_sx = Some(sx);
        ImageManager::init_with_threads(&self.image_manager, info.app_sx.clone());
        Some(std::thread::spawn(move || {
            let mut cached_filter = Option::<Filters>::None;
            let mut cached_search = String::new();

            let anime_generator = StreamableRunner::new()
                // .with_batch_size(1000)
                .change_batch_size_at(1000, 1)
                .stop_early()
                .stop_at(20);

            for animes in anime_generator
                .run(|offset, limit| info.mal_client.get_anime_list(None, offset, limit))
            {
                let anime_ids = animes.iter().map(|a| a.id).collect::<Vec<_>>();
                let update = BackgroundUpdate::new(id.clone())
                    .set("animes", animes)
                    .set("anime_ids", anime_ids)
                    .set("fetching", false)
                    .set("extend", true);
                info.app_sx.send(Event::BackgroundNotice(update)).ok();
            }

            let update = BackgroundUpdate::new(id.clone()).set("startup", false);
            info.app_sx.send(Event::BackgroundNotice(update)).ok();

            while let Ok(_event) = rx.recv() {
                match _event {
                    LocalEvent::Dropdown(animes, filters) => {
                        cached_filter = Some(filters.clone());
                        let mut filtered_animes = animes;
                        Self::filter_animes(&mut filtered_animes, &filters);

                        if !cached_search.is_empty() {
                            Self::search_animes(&mut filtered_animes, cached_search.clone());
                        }

                        // extract just the ids
                        let filtered_animes = filtered_animes
                            .into_iter()
                            .map(|a| a.id)
                            .collect::<Vec<_>>();

                        let update = BackgroundUpdate::new(id.clone())
                            .set("filtered_animes", filtered_animes);
                        info.app_sx.send(Event::BackgroundNotice(update)).ok();
                    }

                    LocalEvent::Search(animes, search) => {
                        let latest_search = search;
                        let mut latest_animes = animes;

                        // for delayed search (if wanted)
                        // while let Ok(_event) = rx.recv_timeout(Duration::from_millis(250)) {
                        //     if let LocalEvent::Search(animes, search) = _event {
                        //         latest_search = search;
                        //         latest_animes = animes;
                        //     }
                        // }

                        if let Some(filters) = cached_filter.clone() {
                            Self::filter_animes(&mut latest_animes, &filters);
                        }

                        cached_search = latest_search.clone();
                        Self::search_animes(&mut latest_animes, latest_search);

                        // extract just the ids
                        let searched_anime =
                            latest_animes.into_iter().map(|a| a.id).collect::<Vec<_>>();

                        let update = BackgroundUpdate::new(id.clone())
                            .set("filtered_animes", searched_anime);
                        info.app_sx.send(Event::BackgroundNotice(update)).ok();
                    }
                }
            }
        }))
    }

    fn apply_update(&mut self, mut update: super::BackgroundUpdate) {
        match (
            update.take::<Vec<AnimeId>>("anime_ids"),
            update.take::<bool>("extend"),
        ) {
            (Some(ids), Some(true)) => {
                self.all_animes.extend(ids);
            }
            (Some(ids), _) => {
                self.all_animes = ids;
                self.navigatable.back_to_start();
            }
            _ => {}
        }

        if self.bg_startup {
            self.filtered_animes = self.all_animes.clone();
        }

        if let Some(startup) = update.take::<bool>("startup") {
            self.bg_startup = startup;
        }

        if let Some(filtered_animes) = update.take::<Vec<AnimeId>>("filtered_animes") {
            self.filtered_animes = filtered_animes;
            self.navigatable.back_to_start();
        }

        if let Some(fetching) = update.take::<bool>("fetching") {
            self.bg_fetching = fetching;
        }
    }
}
