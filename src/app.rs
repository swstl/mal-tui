use crate::config::Config;
use crate::handlers::get_handlers;
use crate::mal::MalClient;
use crate::mal::models::WatchHistory;
use crate::mal::models::anime::Anime;
use crate::mal::models::anime::AnimeId;
use crate::screens::BackgroundUpdate;
use crate::screens::ScreenManager;
use crate::utils::errorBus;
use crate::utils::store::Store;
use crate::player;

use database::DatabaseManager;
use chrono::DateTime;
use chrono::Local;
use chrono::Utc;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableMouseCapture;
use image::DynamicImage;
use ratatui::DefaultTerminal;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;

#[derive(Debug, Clone)]
pub struct ExtraInfo {
    pub app_sx: mpsc::Sender<Event>,
    pub mal_client: Arc<MalClient>,
    pub anime_store: Store<Anime>,
    pub local_db: DatabaseManager,
}

// these are retured when a screen handles an input
#[derive(Debug, Clone)]
pub enum Action {
    PlayAnime(AnimeId),
    PlayEpisode(AnimeId, u32),
    SwitchScreen(&'static str),
    ShowOverlay(AnimeId),
    NavbarSelect(bool),
    ShowError(String),
    SyncAnimes(Vec<(bool, Anime)>),
    Quit,
}

// here will all the details of a specific anime or manga be stored.
#[allow(dead_code)]
pub enum CurrentInfo {
    Anime,
    Manga,
}

// these are sent over the channel at any time
#[allow(dead_code)]
pub enum Event {
    SyncStatus(bool, bool, Box<Anime>),
    Input(crossterm::event::Event),
    KeyPress(crossterm::event::KeyEvent),
    MouseClick(crossterm::event::MouseEvent),
    Resize(u16, u16),
    BackgroundNotice(BackgroundUpdate),
    ImageCached(usize, DynamicImage),
    StorageUpdate(AnimeId, Box<dyn FnOnce(&mut Anime) + Send>),
    ShowError(String),
    Rerender,
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::KeyPress(key_event) => f
                .debug_struct("KeyPress")
                .field("code", &key_event.code)
                .field("modifiers", &key_event.modifiers)
                .finish(),
            Event::MouseClick(mouse_event) => f
                .debug_struct("MouseClick")
                .field("kind", &mouse_event.kind)
                .field("column", &mouse_event.column)
                .field("row", &mouse_event.row)
                .finish(),
            Event::Resize(width, height) => f
                .debug_struct("Resize")
                .field("width", width)
                .field("height", height)
                .finish(),
            Event::BackgroundNotice(_) => f.debug_struct("BackgroundNotice").finish(),
            Event::ImageCached(index, _) => {
                f.debug_struct("ImageCached").field("index", index).finish()
            }
            Event::StorageUpdate(anime_id, _) => f
                .debug_struct("StorageUpdate")
                .field("anime_id", anime_id)
                .finish(),
            Event::ShowError(message) => f
                .debug_struct("ShowError")
                .field("message", message)
                .finish(),
            Event::Rerender => f.debug_struct("Rerender").finish(),
            _ => f.debug_struct("OtherEvent").finish(),
        }
    }
}

#[allow(dead_code)]
pub struct App {
    mal_client: Arc<MalClient>,
    screen_manager: ScreenManager,
    current_info: Option<CurrentInfo>,
    is_running: bool,
    terminal: DefaultTerminal,
    shared_info: ExtraInfo,
    anime_player: player::AnimePlayer,

    sx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    threads: Vec<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl App {
    pub fn new(terminal: DefaultTerminal) -> App {
        let (sx, rx) = mpsc::channel::<Event>();
        errorBus::init(sx.clone());

        let db_path = Config::data_dir()
            .join("log.db")
            .to_string_lossy()
            .to_string();
        let db_manager =
            DatabaseManager::new(db_path).expect("Failed to initialize database manager");
        let mal_client = Arc::new(MalClient::new(db_manager.clone()));
        let universal_info = ExtraInfo {
            app_sx: sx.clone(),
            mal_client: mal_client.clone(),
            anime_store: Store::new(),
            local_db: db_manager,
        };

        App {
            mal_client: mal_client.clone(),
            screen_manager: ScreenManager::new(universal_info.clone()),
            anime_player: player::AnimePlayer::new(),
            current_info: None,
            shared_info: universal_info,
            is_running: true,
            terminal,

            rx,
            sx,
            threads: Vec::new(),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        // run any background threads
        self.spawn_background();

        // WARNING: don't use just unwrap
        while self.is_running {
            self.terminal
                .draw(|frame| self.screen_manager.render_screen(frame))?;

            let first_event = self.rx.recv().unwrap();
            let mut events = vec![first_event];

            // in case multiple events happen at the same time, we want to process them all
            while let Ok(event) = self.rx.try_recv() {
                events.push(event);
            }

            for event in events {
                match event {
                    Event::Input(input_event) => {
                        self.handle_input(input_event);
                    }
                    Event::BackgroundNotice(mut update) => {
                        if let Some(animes) = update.take::<Vec<Anime>>("animes") {
                            self.shared_info.anime_store.add_bulk(animes);
                        }
                        if let Some(sync_animes) = update.take::<Vec<Anime>>("sync") {
                            self.screen_manager.interactive_sync(sync_animes);
                        }

                        self.screen_manager.update_screen(update);
                    }
                    Event::StorageUpdate(anime, updater) => {
                        self.shared_info
                            .anime_store
                            .update(anime, |anime_to_update| {
                                updater(anime_to_update);
                            });
                        self.screen_manager.refresh();
                    }
                    Event::ShowError(message) => {
                        self.screen_manager.show_error(message);
                    }
                    Event::SyncStatus(success, sync, anime ) => {
                        if success {
                            if sync {
                                self.screen_manager.syncing_popup.finished_syncing(*anime);
                            }
                            else{
                                self.screen_manager.syncing_popup.finished_deleting(*anime);
                            }
                        }
                        else {
                            self.screen_manager.show_error(format!("Failed to sync {}", anime.title));
                        }

                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn log_watched_info(&self, anime: &Anime, details: player::PlayResult) {
        let now: DateTime<Local> = Local::now();
        let history = WatchHistory {
            id: 0,
            anime_id: anime.id,
            timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            episode: details.episode as i32,
            current_time: details.current_time,
            total_time: details.total_time,
            percentage: details.percentage,
            is_completed: details.is_completed,
        };
        self.shared_info.local_db.create_table::<Anime>().ok();
        self.shared_info.local_db.create_table::<WatchHistory>().ok();
        self.shared_info.local_db.upsert(anime.clone()).ok();
        self.shared_info.local_db.upsert(history).ok();
    }

    fn play_anime(&mut self, anime_id: AnimeId, episode: u32) -> Option<()> {
        let anime = self.shared_info.anime_store.get(&anime_id)?;

        let next_episode = if episode == 0 {
            std::cmp::min(
                anime.my_list_status.num_episodes_watched + 1,
                anime.num_episodes,
            )
        } else {
            episode
        };

        crossterm::execute!(std::io::stderr(), DisableMouseCapture).ok();

        match self
            .anime_player
            .play_anime(&anime, next_episode)
        {
            Ok(episode_details) => {
                // update the status to now watching (in memory first)
                self.shared_info
                    .anime_store
                    .update(anime.id, |anime_to_update| {
                        anime_to_update.my_list_status.status = "watching".into();
                    });

                if episode_details.is_completed {
                    // update the number of episodes watched 
                    self.shared_info
                        .anime_store
                        .update(anime.id, |anime_to_update| {
                            if anime_to_update.my_list_status.num_episodes_watched
                                < anime_to_update.num_episodes
                            {
                                anime_to_update.my_list_status.num_episodes_watched += 1;
                            } else {
                                anime_to_update.my_list_status.status = "completed".into();
                            }
                            anime_to_update.my_list_status.updated_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(); 
                        });
                }

                // get the anime with the updated details from above
                let updated = self.shared_info.anime_store.get(&anime.id)?;

                // now update this to the mal servers if the user is logged in
                if MalClient::user_is_logged_in(){
                    self.shared_info
                        .mal_client
                        .update_user_list_async((*updated).clone());
                }
                self.screen_manager.refresh();
                self.log_watched_info(&updated, episode_details);
            }
            Err(e) => {
                self.screen_manager.show_error(e.to_string());
            }
        }

        crossterm::execute!(std::io::stderr(), EnableMouseCapture).ok();
        self.terminal = ratatui::init();
        None
    }

    fn handle_input(&mut self, event: crossterm::event::Event) {
        // quit the app on ctrl+c
        if let crossterm::event::Event::Key(key_event) = event
            && key_event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            && key_event.kind == crossterm::event::KeyEventKind::Press
            && key_event.code == crossterm::event::KeyCode::Char('c')
        {
            self.is_running = false;
        }

        let result = self.screen_manager.handle_input(event);
        if let Some(action) = result {
            match action {
                Action::SwitchScreen(screen_name) => {
                    self.screen_manager.change_screen(screen_name);
                }
                Action::ShowOverlay(anime_id) => {
                    self.screen_manager.toggle_overlay(anime_id);
                }
                Action::NavbarSelect(selected) => {
                    self.screen_manager.toggle_navbar(selected);
                }
                Action::PlayAnime(anime_id) => {
                    self.play_anime(anime_id, 0);
                }
                Action::PlayEpisode(anime_id, episode) => {
                    self.play_anime(anime_id, episode);
                }
                Action::ShowError(message) => {
                    self.screen_manager.show_error(message);
                }
                Action::SyncAnimes(animes) => {
                    let tx = self.sx.clone();
                    let client = self.mal_client.clone();
                    let local_db = self.shared_info.local_db.clone();

                    tokio::spawn(async move {
                        for (sync, anime) in animes {
                            if sync {
                                let result = client.update_user_list_async(anime.clone()).await;
                                let success = result.map(|r| r.is_ok()).unwrap_or(false);
                                let _ = tx.send(Event::SyncStatus(success, true,  Box::new(anime)));
                            } else {
                                let success = local_db.delete(&anime).is_ok();
                                let _ = tx.send(Event::SyncStatus(success, false, Box::new(anime)));
                            }
                        }
                    });
                }

                Action::Quit => {
                    self.is_running = false;
                }
            }
        }
    }

    fn spawn_background(&mut self) {
        for handler in get_handlers() {
            let _sx = self.sx.clone();
            let _thread = thread::spawn(move || {
                handler(_sx);
            });
            self.threads.push(_thread);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // restore terminal
        ratatui::restore();
        crossterm::execute!(std::io::stderr(), DisableMouseCapture).ok();
    }
}
