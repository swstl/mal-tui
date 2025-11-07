use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::widgets::animebox::AnimeBox;
use super::widgets::navigatable::Navigatable;
use super::{BackgroundUpdate, ExtraInfo, Screen};
use crate::app::{Action, Event};
use crate::config::Config;
use crate::config::navigation::NavDirection;
use crate::mal::models::anime::AnimeId;
use crate::utils::functionStreaming::StreamableRunner;
use crate::utils::imageManager::ImageManager;
use crossterm::event::KeyEvent;
use indexmap::IndexSet;
use ratatui::layout::{Margin, Rect};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Clear},
};
use tui_widgets::big_text::{BigText, PixelSize};

#[derive(PartialEq, Clone)]
enum Focus {
    NavBar,
    Content,
}

#[derive(Clone)]
struct List {
    title: String,
    navigatable: Navigatable,
    items: Vec<AnimeId>,
    image_manager: Arc<Mutex<ImageManager>>,
}

#[derive(Clone)]
pub struct OverviewScreen {
    bg_loaded: bool,
    app_info: ExtraInfo,

    navigation: Navigatable,
    lists: Vec<List>,
    focus: Focus,
}

impl OverviewScreen {
    pub fn new(info: ExtraInfo) -> Self {
        Self {
            app_info: info,
            bg_loaded: false,
            navigation: Navigatable::new((3, 1)),
            lists: vec![
                List {
                    title: "Recently Watched".to_string(),
                    navigatable: Navigatable::new((1, 5)),
                    items: vec![],
                    image_manager: Arc::new(Mutex::new(ImageManager::new())),
                },
                List {
                    title: "Suggested Animes".to_string(),
                    navigatable: Navigatable::new((1, 5)),
                    items: vec![],
                    image_manager: Arc::new(Mutex::new(ImageManager::new())),
                },
                List {
                    title: "Most Popular".to_string(),
                    navigatable: Navigatable::new((1, 5)),
                    items: vec![],
                    image_manager: Arc::new(Mutex::new(ImageManager::new())),
                },
            ],
            focus: Focus::Content,
        }
    }
}

impl Screen for OverviewScreen {
    // add_screen_caching!();

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);

        let [_, content] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Fill(1)])
            .areas(area);

        // this is the outer navigatable meaning it splits into the vertical thre sections
        self.navigation
            .construct_mut(&mut self.lists, content, |list, area, highlighted| {
                let area = Rect::new(
                    area.x,
                    area.y + 3,
                    area.width,
                    area.height.saturating_sub(2),
                );

                // determine the highlighted color
                let color = if highlighted && self.focus == Focus::Content {
                    Config::global().theme.highlight
                } else {
                    Config::global().theme.primary
                };

                // draw a box for the highlighted section
                let block = Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(color));
                frame.render_widget(
                    block,
                    area.inner(Margin {
                        vertical: 1,
                        horizontal: 3,
                    }),
                );

                // split into title and list area (for each list section)
                let [title_area, list_area, _] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Fill(1),
                        Constraint::Length(1),
                    ])
                    .areas(area.inner(Margin {
                        vertical: 0,
                        horizontal: 8,
                    }));

                // add margin to the title
                let [_, title_area] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Fill(1), Constraint::Length(3)])
                    .areas(title_area);

                let title = BigText::builder()
                    .style(Style::new().fg(color))
                    .pixel_size(PixelSize::Sextant)
                    .lines(vec![list.title.clone().into()])
                    .build();

                frame.render_widget(title, title_area);

                // lett the user know nothin gis there yet if the list is empty
                if list.items.is_empty() {
                    let text = "Nothing here yet!";
                    let paragraph = Paragraph::new(text)
                        .style(Style::default().fg(Color::Red))
                        .wrap(Wrap { trim: true });
                    frame.render_widget(paragraph, list_area);
                    return;
                }

                // this is the inner navigatable (the vertical sections)
                list.navigatable.construct(
                    &list.items,
                    list_area,
                    |anime_id, inner_area, inner_highlighted| {
                        if let Some(anime) = &self.app_info.anime_store.get(anime_id) {
                            AnimeBox::render(
                                anime,
                                &list.image_manager,
                                frame,
                                inner_area.inner(Margin {
                                    vertical: 0,
                                    horizontal: 3,
                                }),
                                inner_highlighted && highlighted && self.focus == Focus::Content,
                            )
                        }
                    },
                );
            });
    }

    fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        let modifier = key_event
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        let nav = &Config::global().navigation;

        match self.focus {
            Focus::NavBar => {
                self.focus = Focus::Content;
            }

            Focus::Content => {
                if modifier && nav.get_direction(&key_event.code) == NavDirection::Up {
                    self.focus = Focus::NavBar;
                    return Some(Action::NavbarSelect(true));
                }

                match nav.get_direction(&key_event.code) {
                    NavDirection::Down => {
                        self.navigation.move_down();
                    }
                    NavDirection::Up => {
                        self.navigation.move_up();
                    }
                    NavDirection::Right => {
                        if let Some(selected) =
                            self.navigation.get_selected_item_mut(&mut self.lists)
                        {
                            selected.navigatable.move_right();
                        }
                    }
                    NavDirection::Left => {
                        if let Some(selected) =
                            self.navigation.get_selected_item_mut(&mut self.lists)
                        {
                            selected.navigatable.move_left();
                        }
                    }
                    _ => {}
                }

                if nav.is_select(&key_event.code)
                    && let Some(selected) = self.navigation.get_selected_item_mut(&mut self.lists)
                    && let Some(anime_id) = selected.navigatable.get_selected_item(&selected.items)
                {
                    return Some(Action::ShowOverlay(*anime_id));
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

        // this happens only when the cursor is hovering over the overview content
        let item = self
            .navigation
            .get_hovered_item_mut(&mut self.lists, mouse_event)?;
        self.focus = Focus::Content;

        // handle scrolling per list
        item.navigatable.handle_scroll(mouse_event);

        // retreive id of the anime being hovered when clicked
        let anime_id = item
            .navigatable
            .get_hovered_item(&item.items, mouse_event)?;
        if let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind {
            return Some(Action::ShowOverlay(*anime_id));
        }

        None
    }

    fn background(&mut self) -> Option<JoinHandle<()>> {
        let already_loaded = self.bg_loaded;
        for item in self.lists.iter_mut() {
            ImageManager::init_with_threads(&item.image_manager, self.app_info.app_sx.clone());
        }
        let info = self.app_info.clone();
        let id = self.get_name();
        let sender = info.app_sx.clone();
        let app_dir = Config::data_dir();
        let log_file = app_dir.join("watch_history");

        Some(thread::spawn(move || {
            let anime_generator = StreamableRunner::new().stop_at(1);

            let mut cached_ids: Vec<AnimeId> = Vec::new();

            if let Ok(file) = OpenOptions::new().read(true).open(log_file) {
                // then we fetch the animes data from the mal api (this is just the users list as
                // the watchd animes will allways be in the users list after a watch)
                // this information will just be handled by the app and the store, and will not be
                // retrieved in this local apply_update

                // this is the users list of animes
                for animes in anime_generator
                    .run(|offset, limit| info.mal_client.get_anime_list(None, offset, limit))
                {
                    cached_ids.extend(animes.iter().map(|a| a.id));
                    let update = BackgroundUpdate::new(id.clone()).set("animes", animes);
                    info.app_sx.send(Event::BackgroundNotice(update)).ok();
                }

                // this is first to fetch the file where the recent watched animes are
                let content = BufReader::new(file);
                let entries: Vec<String> = content.lines().map_while(Result::ok).collect();
                let mut animes = IndexSet::new();

                for entry in entries.iter().rev() {
                    let parts: Vec<&str> = entry.split(" -> ").collect();
                    if parts.len() < 7 {
                        // unexpected format, skip this entry
                        continue;
                    }

                    // idk what to do with this inforamiton yet but here it is.
                    let (
                        _timestamp,
                        anime_id,
                        _title,
                        _episode,
                        _watched_time,
                        _percentage,
                        _completed,
                    ) = (
                        parts[0].to_string(),
                        parts[1].parse::<AnimeId>().expect("Failed to read history"),
                        parts[2].to_string(),
                        parts[3].to_string(),
                        parts[4].to_string(),
                        parts[5].to_string(),
                        parts[6].to_string(),
                    );

                    // save each individual anime id to the set
                    if !cached_ids.contains(&anime_id) {
                        // if the anime is not in the users list, skip it cus they removed it
                        continue;
                    }
                    animes.insert(anime_id);
                }

                let animes: Vec<AnimeId> = animes.into_iter().collect();
                let update = BackgroundUpdate::new(id.clone()).set("WatchHistory", animes);
                sender.send(Event::BackgroundNotice(update)).ok();

                if already_loaded {
                    return;
                }
            }

            // this is the suggested animes
            for animes in anime_generator
                .run(|offset, limit| info.mal_client.get_suggested_anime(offset, limit))
            {
                let anime_ids = animes.iter().map(|a| a.id).collect::<Vec<_>>();
                let update = BackgroundUpdate::new(id.clone())
                    .set("animes", animes)
                    .set("SuggestedAnime", anime_ids);
                info.app_sx.send(Event::BackgroundNotice(update)).ok();
            }

            // this is the most popular animes
            for animes in anime_generator.run(|offset, limit| {
                info.mal_client
                    .get_top_anime("bypopularity".to_string(), offset, limit)
            }) {
                let anime_ids = animes.iter().map(|a| a.id).collect::<Vec<_>>();
                let update = BackgroundUpdate::new(id.clone())
                    .set("animes", animes)
                    .set("PopularAnime", anime_ids);
                info.app_sx.send(Event::BackgroundNotice(update)).ok();
            }
        }))
    }

    fn apply_update(&mut self, mut update: BackgroundUpdate) {
        if let Some(watch_history) = update.take::<Vec<AnimeId>>("WatchHistory") {
            self.lists[0].items = watch_history;
        }

        if let Some(suggested_anime) = update.take::<Vec<AnimeId>>("SuggestedAnime") {
            self.lists[1].items = suggested_anime;
        }

        if let Some(popular_anime) = update.take::<Vec<AnimeId>>("PopularAnime") {
            self.lists[2].items = popular_anime;
        }
    }
}
