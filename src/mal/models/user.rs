use crate::{mal::{Fetchable, network::{Identifier, fetch_user}}, utils::imageManager::HasDisplayableImage};

use serde::{Deserialize, Serialize};

use super::anime::{Anime, FavoriteAnime};

fn default_picture() -> String {
    "https://dogfetus.no/image/pfp".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    #[serde(default)]
    pub id: usize,
    #[serde(default)]
    pub name: String,
    #[serde(default= "default_picture")]
    pub picture: String,
    #[serde(default)]
    pub gender: String,
    #[serde(default)]
    pub birthday: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub joined_at: String,
    #[serde(default)]
    pub anime_statistics: AnimeStatistics,
    #[serde(default)]
    pub time_zone: String,
    #[serde(default)]
    pub is_supporter: bool,
    #[serde(default)]
    pub favorited_animes: Vec<FavoriteAnime>,
    #[serde(default)]
    pub listed_animes: Vec<Anime>,
    #[serde(default)]
    pub user_stats: UserStatistics,
}

impl User {
    pub fn empty() -> Self {
        Self {
            id: 0,
            name: String::new(),
            picture: String::new(),
            gender: String::new(),
            birthday: String::new(),
            location: String::new(),
            joined_at: String::new(),
            anime_statistics: AnimeStatistics::default(),
            time_zone: String::new(),
            is_supporter: false,
            favorited_animes: Vec::new(),
            listed_animes: Vec::new(),
            user_stats: UserStatistics::default(),
        }
    }

    pub fn add_favorite_animes(&mut self, animes: Vec<FavoriteAnime>) {
        self.favorited_animes.extend(animes);
    }
    pub fn add_listed_animes(&mut self, animes: Vec<Anime>) {
        animes.iter().for_each(|anime| {
            if anime.my_list_status.score > 0 {
                self.user_stats.num_items_rated += 1;
            }
        });
        self.listed_animes.extend(animes);
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnimeStatistics {
    #[serde(default)]
    pub num_items_watching: usize,
    #[serde(default)]
    pub num_items_completed: usize,
    #[serde(default)]
    pub num_items_on_hold: usize,
    #[serde(default)]
    pub num_items_dropped: usize,
    #[serde(default)]
    pub num_items_plan_to_watch: usize,
    #[serde(default)]
    pub num_items: usize,
    #[serde(default)]
    pub num_days_watched: f64,
    #[serde(default)]
    pub num_days_watching: f64,
    #[serde(default)]
    pub num_days_completed: f64,
    #[serde(default)]
    pub num_days_on_hold: f64,
    #[serde(default)]
    pub num_days_dropped: f64,
    #[serde(default)]
    pub num_days: f64,
    #[serde(default)]
    pub num_episodes: usize,
    #[serde(default)]
    pub num_times_rewatched: usize,
    #[serde(default)]
    pub mean_score: f64,
}

impl Default for AnimeStatistics {
    fn default() -> Self {
        Self {
            num_items_watching: 0,
            num_items_completed: 0,
            num_items_on_hold: 0,
            num_items_dropped: 0,
            num_items_plan_to_watch: 0,
            num_items: 0,
            num_days_watched: 0.0,
            num_days_watching: 0.0,
            num_days_completed: 0.0,
            num_days_on_hold: 0.0,
            num_days_dropped: 0.0,
            num_days: 0.0,
            num_episodes: 0,
            num_times_rewatched: 0,
            mean_score: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct UserStatistics {
    #[serde(default)]
    pub num_items_rated: usize,
}

impl Fetchable for User {
    type Response = Self;
    type Output = Self;

    fn fetch(
        identifier: Identifier,
        url: String,
        parameters: Vec<(String, String)>,
    ) -> Result<Self::Response, Box<dyn std::error::Error>> {
        fetch_user(identifier, url, parameters)
    }

    fn from_response(response: Self::Response) -> Self::Output {
        response
    }

}

impl HasDisplayableImage for User {
    fn get_displayable_image(&self) -> Option<(usize, String)> {
        Some((self.id, self.picture.clone()))
    }
}
