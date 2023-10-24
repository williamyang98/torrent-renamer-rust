use tokio;
use tvdb;
use app;
use anyhow;
use futures;
use reqwest;
use serde;
use serde_json;

use tvdb::api::LoginSession;
use app::file_intent::{Action, FilterRules};
use app::app_folder::AppFolder;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct Credentials {
    #[serde(rename="credentials")]
    pub login_info: tvdb::api::LoginInfo,     
    pub token: String,
}

async fn test_folder(root_path: String, filter_rules: &FilterRules, login_session: Option<&LoginSession<'_>>, is_execute: bool, is_aggressive: bool) -> Result<(), anyhow::Error> {
    let mut folder = AppFolder::new(root_path.as_str(), &filter_rules);
    folder.load_cache_from_file().await?;    
    folder.update_file_intents().await?;
    folder.flush_file_changes();
    
    if false {
        if let Some(login_session) = login_session.as_ref() {
            let series_id: u32 = 248742;
            folder.load_cache_from_api(login_session, series_id).await?;
            folder.update_file_intents().await?;
            folder.flush_file_changes();
        };
    }
    
    if is_aggressive {
        for file_index in 0..folder.get_total_files() {
            if let Some(mut file) = folder.get_file(file_index) {
                if file.get_action() == Action::Ignore || file.get_action() == Action::Delete {
                    file.set_is_enabled(true);
                    file.set_action(Action::Delete);
                }
            }
        }
        folder.flush_file_changes();
    }
    
    {
        let conflict_table = folder.get_conflict_table();
        let action_count = conflict_table.get_action_count();
        println!("#### FOLDER: {} ####", root_path);
        println!("    counts={:?}", action_count);
    }

    let print_actions: Vec<Action> = vec![Action::Complete, Action::Whitelist];
    // let print_actions: Vec<Action> = vec![];
    println!("    src={}", folder.get_total_files());
    for file_index in 0..folder.get_total_files() {
        if let Some(file) = folder.get_file(file_index) {
            if !print_actions.contains(&file.get_action()) {
                println!("    {:?}", file);
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

            let src_file = match conflict_table.get_source_index(dest.as_str()) {
                None => None,
                Some(index) => match folder.get_file(*index) {
                    Some(file) => Some(file),
                    None => None,
                },
            };

            let mut total_target_writes = entries.len();
            if src_file.is_some() {
                total_target_writes += 1;
            }

            if total_target_writes <= 1 {
                continue;
            }

            println!("    dest={:?}", dest.as_str());
            if let Some(file) = &src_file {
                println!("        > {:?}", file);
            }
            for index in entries {
                if let Some(file) = folder.get_file(*index) {
                    println!("        | {:?}", file);
                }
            }
        }
    }
    
    if is_execute {
        folder.execute_file_changes().await;
        folder.delete_empty_folders().await;
    }

    // folder.save_cache_to_file().await?;
    Ok(())
}

async fn test(root_path: &str, is_single_folder: bool, is_execute: bool, is_aggressive: bool) -> Result<(), anyhow::Error> {
    let config_path = "C:/tools/torrent-renamer-cpp/res";

    let (filter_rules_str, credentials_str) = tokio::join!(
        tokio::fs::read_to_string(format!("{}/app_config.json", config_path)),
        tokio::fs::read_to_string(format!("{}/credentials.json", config_path)),
    );

    let filter_rules: FilterRules = serde_json::from_str(filter_rules_str?.as_str())?;
    let credentials: Credentials = serde_json::from_str(credentials_str?.as_str())?;
    println!("{:?}", filter_rules);
    println!("{:?}", credentials);

    if false {
        let client = reqwest::Client::new();
        let token = tvdb::api::login(&client, &credentials.login_info).await?;
        println!("{:?}", token);
        let session = tvdb::api::LoginSession::new(&client, &token);
    };

    if !is_single_folder {
        let mut tasks = Vec::new();
        let mut entries = tokio::fs::read_dir(root_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if !file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            if let Some(path) = path.to_str() {
                let task = test_folder(path.to_string(), &filter_rules, None, is_execute, is_aggressive);
                tasks.push(task);
            }
        }

        for res in futures::future::join_all(tasks).await.into_iter() {
            if let Err(err) = res {
                println!("{:?}", err);
            };
        }
        return Ok(());
    } 

    test_folder(root_path.to_string(), &filter_rules, None, is_execute, is_aggressive).await
}

fn print_usage(name: &str) {
    println!("Usage: {} <filepath> [--multiple] [--execute] [--aggressive] [--help]", name);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        print_usage(args[0].as_str());
        return;
    };
    
    let root_path = args[1].as_str();

    let mut is_single_folder = true;
    let mut is_aggressive = false;
    let mut is_execute = false;
    
    let args = args.as_slice();
    for arg in &args[2..] {
        let arg = arg.as_str();
        match arg {
            "--multiple" => is_single_folder = false,
            "--execute" => is_execute = true,
            "--aggressive" => is_aggressive = true,
            "--help" => {
                print_usage(args[0].as_str());
                return;
            },
            arg => {
                println!("Bad option: {}", arg);
                print_usage(args[0].as_str());
                return;
            },
        }
    }

    if let Err(err) = test(root_path, is_single_folder, is_execute, is_aggressive).await {
        println!("{:?}", err);
    }
}
