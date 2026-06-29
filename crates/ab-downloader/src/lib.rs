pub mod client;
pub mod error;
pub mod mock;
pub mod path;
pub mod qb;
pub mod aria2;

pub use client::{DownloadClient, DownloaderClient, TorrentInfo, create_client, init_downloader};
pub use error::DownloaderError;
pub use path::{
    check_files, file_depth, gen_save_path, is_ep, join_path, path_to_bangumi, rule_name, FileInfo,
};
