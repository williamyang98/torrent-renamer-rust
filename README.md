# Introduction
[![x86-windows](https://github.com/williamyang98/torrent-renamer-rust/actions/workflows/x86-windows.yml/badge.svg)](https://github.com/williamyang98/torrent-renamer-rust/actions/workflows/x86-windows.yml)
[![x86-ubuntu](https://github.com/williamyang98/torrent-renamer-rust/actions/workflows/x86-ubuntu.yml/badge.svg)](https://github.com/williamyang98/torrent-renamer-rust/actions/workflows/x86-ubuntu.yml)

A torrent renaming tool built in rust
- Uses TVDB database for renaming files with correct names
- Uses regex search for finding candidates for renaming
- Deletes blacklisted extensions

## Preview
![Main window](docs/screenshot_v1.png)

## Credentials
For both the gui app and cli scripts, you need to supply your TVDB api credentials. 
See "res/example-credentials.json" for the json template.
The default path that is read is "credentials.json".

### Getting credentials from dashboard
You can check out the [tvdb dashboard](https://thetvdb.com/dashboard) for your api information. This is required for performing api requests.

![alt text](docs/credentials_user_v2.png "Username and userkey in dashboard")
![alt text](docs/credentials_api_v2.png "Apikey in dashboard")

## Building
1. Install Rust.
2. ```cargo build -r```.
3. ```cargo run -r```.

## C++ version
The original C++ version of this application can be found [here](https://github.com/williamyang98/TorrentRenamerCpp). 
Significant improvements were made using reqwests and tokio::fs for better IO when using network attached storage.