use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};
use std::{
    sync::{Arc, Mutex},
};
// Assuming these imports exist based on your provided code
use crate::{
    app::{Action, ExtraInfo},
    config::{Config, navigation::NavDirection},
    mal::models::anime::Anime,
    screens::widgets::navigatable::Navigatable,
    utils::imageManager::ImageManager,
};
use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusedElement {
    SyncButtons,
    BatchButton
}

#[derive(Clone)]
pub struct SyncPopup {
    toggled: bool,
    syncing: bool,
    animes_to_sync: Vec<Anime>,
    info: ExtraInfo,
    focus: FocusedElement,

    // UI Components
    buttons: Vec<String>,
    nav: Navigatable,
    image_manager: Arc<Mutex<ImageManager>>,
    all_selected: bool,
}

impl SyncPopup {
    pub fn new(info: ExtraInfo) -> Self {
        let buttons = vec![
            "Sync to MAL".to_string(),
            "Remove local changes".to_string(),
        ];

        let image_manager = Arc::new(Mutex::new(ImageManager::new()));

        ImageManager::init_with_threads(&image_manager, info.app_sx.clone());

        Self {
            toggled: false,
            syncing: false,
            animes_to_sync: Vec::new(),
            nav: Navigatable::new((1, buttons.len() as u16)), // 2 columns, 1 row
            buttons,
            image_manager,
            info,
            all_selected: false,
            focus: FocusedElement::SyncButtons,
        }
    }

    pub fn open(&mut self) -> &Self {
        if !self.animes_to_sync.is_empty() {
            self.toggled = true;
        }
        self
    }

    fn change_button_lables(&mut self) {
        if self.all_selected {
            self.buttons = vec![
                "Sync to MAL (all)".to_string(),
                "Remove local changes (all)".to_string(),
            ];
        } else {
            self.buttons = vec![
                "Sync to MAL".to_string(),
                "Remove local changes".to_string(),
            ];
        }
    }

    pub fn is_open(&self) -> bool {
        self.toggled
    }

    pub fn close(&mut self) -> &Self {
        self.toggled = false;
        self
    }

    pub fn set_animes(&mut self, animes: Vec<Anime>) -> &Self {
        self.animes_to_sync = animes;
        // If we have animes and we are open, ensure navigation is reset
        if !self.animes_to_sync.is_empty() {
            self.nav.back_to_start();
        }
        self
    }

    /// Helper to get the current anime being viewed
    fn current_anime(&self) -> Option<&Anime> {
        self.animes_to_sync.first()
    }

    pub fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        if !self.toggled {
            return None;
        }

        let nav_config = &Config::global().navigation;

        match self.focus{
            FocusedElement::BatchButton => {
                if nav_config.get_direction(&key_event.code) == NavDirection::Down {
                    self.focus = FocusedElement::SyncButtons;
                }

                if nav_config.is_select(&key_event.code) {
                    self.all_selected = !self.all_selected;
                    self.change_button_lables();
                }
            },

            FocusedElement::SyncButtons => {
                // Navigation
                match nav_config.get_direction(&key_event.code) {
                    NavDirection::Left => self.nav.move_left(),
                    NavDirection::Right => self.nav.move_right(),
                    NavDirection::Up => {
                        self.focus = FocusedElement::BatchButton;
                    }
                    _ => {}
                }

                // Selection
                if nav_config.is_select(&key_event.code)
                    && let Some(anime) = self.current_anime().cloned()
                {
                    match self.nav.get_selected_index() {
                        0 => return Some(Action::SyncAnime(anime)),
                        1 => return Some(Action::DiscardAnime(anime)),
                        _ => {}
                    }
                }

                // Close
                if nav_config.is_close(&key_event.code) {
                    self.close();
                }
            }
        }

        self.animes_to_sync.remove(0);

        None
    }

    pub fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<Action> {
        if !self.toggled {
            return None;
        }

        let is_click = matches!(mouse_event.kind, MouseEventKind::Down(_));

        // Handle buttons
        if self.nav.get_hovered_index(mouse_event).is_some()
            && is_click
            && let Some(anime) = self.current_anime().cloned()
        {
            match self.nav.get_selected_index() {
                0 => return Some(Action::SyncAnime(anime)),
                1 => return Some(Action::DiscardAnime(anime)),
                _ => {}
            }
        }

        None
    }

    pub fn render(&mut self, frame: &mut Frame) {
        if !self.toggled {
            return;
        }

        // If no animes left to sync, close automatically
        let anime = match self.current_anime() {
            Some(a) => a,
            None => {
                self.close();
                return;
            }
        };
        let mal_anime = self.info.anime_store.get(&anime.id);

        let area = frame.area();

        // Increase width slightly to accommodate two columns side-by-side
        let width = std::cmp::min(area.width * 80 / 100, 100);
        let height = std::cmp::min(area.height * 60 / 100, 25);

        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );

        // self.popup_area = Some(popup_area);

        // Clear background
        frame.render_widget(Clear, popup_area);

        // Main Block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ratatui::symbols::border::ROUNDED)
            .title(format!(" Sync Conflict ({}) ", self.animes_to_sync.len()))
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Config::global().theme.secondary));

        frame.render_widget(block.clone(), popup_area);

        // Inner Layout: Top (Content), Bottom (Buttons)
        let inner_area = block.inner(popup_area);
        let [content_area, button_row] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3), // Height for buttons
            ])
            .areas(inner_area);

        // Content Layout: Left (Image), Right (Text Info)
        let [image_area, text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 4), // Image takes ~25%
                Constraint::Ratio(3, 4),
            ])
            .areas(content_area);

        // 1. Render Image
        // Adjust image area for padding
        let img_render_area = image_area.inner(Margin::new(1, 1));
        ImageManager::render_image(&self.image_manager, anime, frame, img_render_area, true);

        // 2. Render Text Info
        let title = if anime.alternative_titles.en.is_empty() {
            &anime.title
        } else {
            &anime.alternative_titles.en
        };

        let [title_area, text_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .areas(text_area);

        let [title_area, all_btn_area, _] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Length(10), Constraint::Length(1)])
            .areas(title_area);

        let [conflict_location, text_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0)])
            .areas(text_area);

        let [local_conflict_area, remote_conflict_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(conflict_location);

        let [local_text_area, remote_text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(text_area);

        let title_text = vec![Line::from(vec![
            Span::styled(
                "Anime: ",
                Style::default().fg(Config::global().theme.secondary),
            ),
            Span::raw(title).bold(),
        ])];

        let conflict_local_text = vec![Line::from(vec![Span::styled(
            "Local save:",
            Style::default().fg(Config::global().theme.highlight),
        )])];

        let conflict_remote_text = vec![Line::from(vec![Span::styled(
            "MAL save:",
            Style::default().fg(Config::global().theme.highlight),
        )])];

        let theme = Config::global().theme.clone();

        let status_color = mal_anime
            .as_ref()
            .filter(|remote| anime.my_list_status.status != remote.my_list_status.status)
            .map(|_| theme.error)
            .unwrap_or(theme.primary);

        let score_color = mal_anime
            .as_ref()
            .filter(|remote| anime.my_list_status.score != remote.my_list_status.score)
            .map(|_| theme.error)
            .unwrap_or(theme.primary);

        let watched_color = mal_anime
            .as_ref()
            .filter(|remote| {
                anime.my_list_status.num_episodes_watched
                    != remote.my_list_status.num_episodes_watched
            })
            .map(|_| theme.error)
            .unwrap_or(theme.primary);

        let local_info_text = vec![
            Line::from(vec![
                Span::raw("Status: "),
                Span::styled(
                    anime.my_list_status.status.to_string(),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(vec![
                Span::raw("Score: "),
                Span::styled(
                    anime.my_list_status.score.to_string(),
                    Style::default().fg(score_color),
                ),
            ]),
            Line::from(vec![
                Span::raw("Watched: "),
                Span::styled(
                    format!(
                        "{}/{}",
                        anime.my_list_status.num_episodes_watched, anime.num_episodes
                    ),
                    Style::default().fg(watched_color),
                ),
            ]),
        ];

        if mal_anime.is_none() {
            // No remote info available
            let no_remote_info = vec![Line::from(vec![Span::styled(
                "Not added to list.",
                Style::default().fg(Config::global().theme.error),
            )])];

            let remote_paragraph = Paragraph::new(no_remote_info)
                .wrap(Wrap { trim: true })
                .block(Block::default().padding(Padding::new(1, 1, 1, 1)));
            frame.render_widget(remote_paragraph, remote_text_area);
        } else {
            let remote_info_text = match mal_anime.as_ref() {
                None => vec![Line::from(vec![Span::styled(
                    "Not added to list.",
                    Style::default().fg(Config::global().theme.error),
                )])],
                Some(remote) => vec![
                    Line::from(vec![
                        Span::raw("Status: "),
                        Span::styled(
                            remote.my_list_status.status.to_string(),
                            Style::default().fg(status_color),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("Score: "),
                        Span::styled(
                            remote.my_list_status.score.to_string(),
                            Style::default().fg(score_color),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("Watched: "),
                        Span::styled(
                            format!(
                                "{}/{}",
                                remote.my_list_status.num_episodes_watched, remote.num_episodes
                            ),
                            Style::default().fg(watched_color),
                        ),
                    ]),
                ],
            };

            let remote_paragraph = Paragraph::new(remote_info_text)
                .wrap(Wrap { trim: true })
                .block(Block::default().padding(Padding::new(1, 1, 1, 1)));

            frame.render_widget(remote_paragraph, remote_text_area);
        }

        let title_paragraph = Paragraph::new(title_text)
            .alignment(Alignment::Left)
            .block(Block::default().padding(Padding::new(1, 1, 1, 0)));
        frame.render_widget(title_paragraph, title_area);

        let conflict_location_paragraph = Paragraph::new(conflict_local_text)
            .alignment(Alignment::Left)
            .block(Block::default().padding(Padding::new(1, 1, 1, 0)));
        frame.render_widget(conflict_location_paragraph, local_conflict_area);

        let conflict_remote_paragraph = Paragraph::new(conflict_remote_text)
            .alignment(Alignment::Left)
            .block(Block::default().padding(Padding::new(1, 1, 1, 0)));
        frame.render_widget(conflict_remote_paragraph, remote_conflict_area);

        let local_paragraph = Paragraph::new(local_info_text)
            .wrap(Wrap { trim: true })
            .block(Block::default().padding(Padding::new(1, 1, 1, 1)));
        frame.render_widget(local_paragraph, local_text_area);

        // 3. Render Buttons
        // We want the buttons centered or spread out.
        // Let's use the Navigatable construct logic you already have.
        let all_btn = if self.all_selected { "plural" } else { "single" };
        let all_btn = Paragraph::new(all_btn)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(ratatui::symbols::border::ROUNDED)
                    .style(Style::default().fg(if self.focus == FocusedElement::BatchButton {
                        Config::global().theme.highlight
                    } else {
                        Config::global().theme.primary
                    })),
            );
        frame.render_widget(all_btn, all_btn_area);

        let buttons_rect = Rect::new(
            button_row.x + 1,
            button_row.y,
            button_row.width.saturating_sub(2),
            button_row.height,
        );

        // Create button labels with " (all)" appended if ctrl is held
        self.nav.construct(
            &self.buttons,
            buttons_rect,
            |button_label, area, highlighted| {
                let style = if highlighted && self.focus == FocusedElement::SyncButtons {
                    Style::default().fg(Config::global().theme.highlight)
                } else {
                    Style::default().fg(Config::global().theme.primary)
                };

                let btn_text = Paragraph::new(button_label.as_str())
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_set(ratatui::symbols::border::ROUNDED)
                            .style(style),
                    );

                frame.render_widget(btn_text, area);
            },
        );
    }
}
