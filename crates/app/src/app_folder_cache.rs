use tvdb::models::{Episode, Series};
use std::collections::HashMap;

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub struct EpisodeKey {
    pub season: u32,
    pub episode: u32,
}

pub struct AppFolderCache {
    pub series: Series,
    pub episodes: Vec<Episode>,
    pub episode_cache: HashMap<EpisodeKey, usize>,
}

impl AppFolderCache {
    pub fn new(series: Series, episodes: Vec<Episode>) -> Self {
        let mut cache = HashMap::new();
        for (index, episode) in episodes.iter().enumerate() {
            let key = EpisodeKey {
                season: episode.season,
                episode: episode.episode,
            };
            cache.insert(key, index);
        }

        Self {
            series,
            episode_cache: cache,
            episodes,
        }
    }
}
