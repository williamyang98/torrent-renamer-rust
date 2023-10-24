use tokio;
use tvdb;
use app;
use anyhow;
use reqwest;
use serde;
use serde_json;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct Credentials {
    #[serde(rename="credentials")]
    pub login_info: tvdb::api::LoginInfo,     
    pub token: Option<String>,
}

async fn test() -> Result<(), anyhow::Error> {
    let credentials: Credentials = {
        let data = tokio::fs::read_to_string("C:/tools/torrent-renamer-cpp/res/credentials.json").await?;
        serde_json::from_str(data.as_str())?
    };

    let client = reqwest::Client::new();
    let token = tvdb::api::login(&client, &credentials.login_info).await?;

    println!("Token: {:?}", token);
    let session = tvdb::api::LoginSession::new(&client, &token);
    use app::file_intent::FilterRules;
    use app::app_folder::AppFolder;

    let root_path = "../TorrentRenamerCpp/tests/series_0/";
    
    let filter_rules: FilterRules = {
        let data = tokio::fs::read_to_string("C:/tools/torrent-renamer-cpp/res/app_config.json").await?;
        serde_json::from_str(data.as_str())?
    };
    println!("{:?}", filter_rules);
    
    let mut folder = AppFolder::new(root_path, &filter_rules);

    if let Err(err) = folder.load_cache_from_file().await {
        println!("Load Cache Error: {:?}", err);
    }
    folder.update_file_intents();
    
    let series_id: u32 = 248742;
    folder.load_cache_from_api(&session, series_id).await?;
    folder.update_file_intents();

    use app::file_intent::Action;

    for file_index in 0..folder.get_total_files() {
        if let Some(mut file) = folder.get_file(file_index) {
            if file.get_action() == Action::Ignore || file.get_action() == Action::Delete {
                file.set_is_enabled(true);
                file.set_action(Action::Delete);
            }
        }
    }
    folder.flush_file_changes();
    
    let print_actions: Vec<Action> = vec![Action::Complete, Action::Whitelist];
    for file_index in 0..folder.get_total_files() {
        if let Some(file) = folder.get_file(file_index) {
            if !print_actions.contains(&file.get_action()) {
                println!("{:?}", file);
            }
        }
    }
    
    {
        let conflict_table = folder.get_conflict_table();
        let pending_writes = conflict_table.get_pending_writes();
        for (dest, entries) in pending_writes {
            if entries.len() == 0usize {
                continue;
            }

            let file = match conflict_table.get_source_index(dest.as_str()) {
                None => None,
                Some(index) => match folder.get_file(*index) {
                    Some(file) => Some(file),
                    None => None,
                },
            };

            let mut total_target_writes = entries.len();
            if file.is_some() {
                total_target_writes += 1;
            }

            if total_target_writes <= 1 {
                continue;
            }

            println!("dest={:?}", dest.as_str());
            if let Some(file) = &file {
                println!("    > {:?}", file);
            }
            for index in entries {
                if let Some(file) = folder.get_file(*index) {
                    println!("    | {:?}", file);
                }
            }
        }
    }
    
    folder.execute_file_changes().await;
    folder.delete_empty_folders().await;

    folder.save_cache_to_file().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = test().await {
        println!("{:?}", err);
    }
}
