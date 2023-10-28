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
    pub fn new(series: Series, mut episodes: Vec<Episode>) -> Self {
        // Sort so that our search results are sorted
        episodes.sort_unstable_by(|a,b| {
            const N: u32 = 1000;
            let v_a = a.season*N + a.episode;
            let v_b = b.season*N + b.episode;
            v_a.partial_cmp(&v_b).unwrap_or(std::cmp::Ordering::Equal)
        });

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
