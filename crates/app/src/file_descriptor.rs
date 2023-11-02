use regex::Regex;
use lazy_static::lazy_static;
use crate::transliterate::transliterate;

#[derive(Debug)]
pub struct FileDescriptor {
    pub title: String,
    pub season: u32,
    pub episode: u32,
    pub tags: Vec<String>,
    pub extension: String,
}

const TITLE_PATTERN: &str = r"([a-zA-Z\.\s\-]*)[^a-zA-Z\.\s\-]*";
const EXT_PATTERN: &str = r"\.([a-zA-Z0-9]+)";

pub fn find_tags(tags_str: &str) -> Vec<String> {
    lazy_static! {
        static ref TAG_REGEX: Regex = Regex::new(r"[\[\(]([a-zA-Z0-9]{2,})[\]\)]").unwrap();
    }
    TAG_REGEX.captures_iter(tags_str)
        .map(|x| x[1].to_string())
        .collect()
}


pub fn get_descriptor(filename: &str) -> Option<FileDescriptor> {
    lazy_static! {
        static ref SEASON_EPISODE_EXT_REGEXES: Vec<Regex> = vec![
            Regex::new(format!("{}{}{}", TITLE_PATTERN, r"[Ss](\d+)\s*[Ee](\d+)(.*)", EXT_PATTERN).as_str()).unwrap(),
            Regex::new(format!("{}{}{}", TITLE_PATTERN, r"[Ss]eason\s*(\d+)\s*[Ee]pisode\s*(\d+)(.*)", EXT_PATTERN).as_str()).unwrap(),
            Regex::new(format!("{}{}{}", TITLE_PATTERN, r"(\d+)\s*x\s*(\d+)(.*)", EXT_PATTERN).as_str()).unwrap(),
            Regex::new(format!("{}{}{}", TITLE_PATTERN, r"[^\w]+(\d)(\d\d)[^\w]+(.*)", EXT_PATTERN).as_str()).unwrap(),
        ];
    }

    for re in SEASON_EPISODE_EXT_REGEXES.iter() {
        if let Some(res) = re.captures(filename) {
            return Some(FileDescriptor {
                title: res[1].to_string(),
                season: res[2].parse().unwrap_or(0),
                episode: res[3].parse().unwrap_or(0),
                tags: find_tags(&res[4]),
                extension: res[5].to_string(),
            });
        }
    }
    None
}

pub fn clean_series_name(value: &str) -> String {
    lazy_static! {
        static ref TAG_REGEX: Regex = Regex::new(r"[\[\(]([a-zA-Z0-9]{2,})[\]\)]").unwrap();
        static ref REMOVE_REGEX: Regex = Regex::new(r"[',\(\)\[\]]").unwrap();
        static ref REPLACE_REGEX: Regex = Regex::new(r"[^a-zA-Z0-9]+").unwrap();
    }
    
    let mut new_value: String = TAG_REGEX.replace_all(value, "").to_string();
    new_value = REMOVE_REGEX.replace_all(new_value.as_str(), "").to_string();
    new_value = REPLACE_REGEX.replace_all(new_value.as_str(), " ").to_string();
    new_value = new_value.trim().replace(' ', ".").to_string();
    new_value
}

pub fn clean_episode_title(value: &str) -> String {
    lazy_static! {
        static ref REMOVE_REGEX: Regex = Regex::new(r"[',\(\)\[\]]").unwrap();
        static ref REMOVE_TAGS: Regex = Regex::new(r"[\[\(].*[\)\]]").unwrap();
        static ref REPLACE_REGEX: Regex = Regex::new(r"[^a-zA-Z0-9]+").unwrap();
    }

    let mut new_value: String = REMOVE_REGEX.replace_all(value, "").to_string();
    new_value = REMOVE_TAGS.replace_all(new_value.as_str(), "").to_string();
    new_value = transliterate(new_value.as_str());
    new_value = REPLACE_REGEX.replace_all(new_value.as_str(), " ").to_string();
    new_value = new_value.trim().replace(' ', ".").to_string();
    new_value
}
