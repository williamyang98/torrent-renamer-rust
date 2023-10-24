use tvdb::models::Episode;
use std::collections::HashMap;

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub struct EpisodeKey {
    pub season: u32,
    pub episode: u32,
}

pub type EpisodeCache = HashMap<EpisodeKey, usize>;

pub fn get_episode_cache(episodes: &[Episode]) -> EpisodeCache {
    let mut cache: EpisodeCache =  EpisodeCache::new();
    for (index, episode) in episodes.iter().enumerate() {
        let key = EpisodeKey {
            season: episode.season,
            episode: episode.episode,
        };
        cache.insert(key, index);
    }
    cache
}



