// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::FilePriority;
use crate::integrations::control::{write_control_request, ControlPriorityTarget, ControlRequest};
use crate::integrations::status::status_file_path;
use clap::{Parser, Subcommand, ValueEnum};
use sha1::{Digest, Sha1};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    pub input: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Add {
        input: String,
    },
    StopClient,
    Journal,
    Status {
        #[arg(long)]
        follow: bool,
        #[arg(long)]
        stop: bool,
    },
    Pause {
        #[arg(value_name = "INFO_HASH_HEX")]
        info_hash: Option<String>,
    },
    Resume {
        #[arg(value_name = "INFO_HASH_HEX")]
        info_hash: Option<String>,
    },
    Delete {
        #[arg(value_name = "INFO_HASH_HEX")]
        info_hash: Option<String>,
    },
    Priority {
        #[arg(value_name = "INFO_HASH_HEX")]
        info_hash: Option<String>,
        #[arg(long, conflicts_with = "file_path")]
        file_index: Option<usize>,
        #[arg(long, conflicts_with = "file_index")]
        file_path: Option<String>,
        priority: CliPriority,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliPriority {
    Normal,
    High,
    Skip,
}

impl From<CliPriority> for FilePriority {
    fn from(value: CliPriority) -> Self {
        match value {
            CliPriority::Normal => FilePriority::Normal,
            CliPriority::High => FilePriority::High,
            CliPriority::Skip => FilePriority::Skip,
        }
    }
}

pub fn write_input_command(input_str: &str, watch_path: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(watch_path)?;

    if input_str.starts_with("magnet:") {
        let hash_bytes = Sha1::digest(input_str.as_bytes());
        let file_hash_hex = hex::encode(hash_bytes);

        let final_filename = format!("{}.magnet", file_hash_hex);
        let final_path = watch_path.join(final_filename);
        let temp_filename = format!("{}.magnet.tmp", file_hash_hex);
        let temp_path = watch_path.join(temp_filename);

        tracing::info!(
            "Attempting to write magnet link to temporary path: {:?}",
            temp_path
        );
        match fs::write(&temp_path, input_str.as_bytes()) {
            Ok(_) => {
                tracing::info!(
                    "Atomically renaming magnet file to final path: {:?}",
                    final_path
                );
                if let Err(e) = fs::rename(&temp_path, &final_path) {
                    tracing::error!("Failed to atomically rename magnet file: {}", e);
                    return Err(e);
                }
                return Ok(final_path);
            }
            Err(e) => {
                tracing::error!("Failed to write magnet file to temporary path: {}", e);
                return Err(e);
            }
        }
    } else {
        let torrent_path = PathBuf::from(input_str);
        match fs::canonicalize(&torrent_path) {
            Ok(absolute_path) => {
                let hash_bytes = Sha1::digest(absolute_path.to_string_lossy().as_bytes());
                let file_hash_hex = hex::encode(hash_bytes);
                let final_filename = format!("{}.path", file_hash_hex);
                let final_dest_path = watch_path.join(final_filename);
                let temp_filename = format!("{}.path.tmp", file_hash_hex);
                let temp_dest_path = watch_path.join(temp_filename);

                let absolute_path_cow = absolute_path.to_string_lossy();
                let content = absolute_path_cow.as_bytes();

                tracing::info!(
                    "Attempting to write torrent path to temporary path: {:?}",
                    temp_dest_path
                );
                match fs::write(&temp_dest_path, content) {
                    Ok(_) => {
                        tracing::info!(
                            "Atomically renaming path file to final path: {:?}",
                            final_dest_path
                        );
                        if let Err(e) = fs::rename(&temp_dest_path, &final_dest_path) {
                            tracing::error!("Failed to atomically rename path file: {}", e);
                            return Err(e);
                        }
                        return Ok(final_dest_path);
                    }
                    Err(e) => {
                        tracing::error!("Failed to write path file to temporary path: {}", e);
                        return Err(e);
                    }
                }
            }
            Err(e) => {
                // Don't treat as error if launched by macOS without a valid path
                if !input_str.starts_with("magnet:") {
                    // Avoid logging error for magnet links here
                    tracing::warn!(
                        "Input '{}' is not a valid torrent file path: {}",
                        input_str,
                        e
                    );
                }
                return Err(io::Error::new(io::ErrorKind::InvalidInput, e));
            }
        }
    }
}

pub fn write_stop_command(watch_path: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(watch_path)?;
    let file_path = watch_path.join("shutdown.cmd");
    fs::write(&file_path, "STOP")?;
    Ok(file_path)
}

pub fn command_to_control_request(command: &Commands) -> Result<Option<ControlRequest>, String> {
    match command {
        Commands::Status { follow, stop } => {
            if *follow && *stop {
                return Err("Choose either --follow or --stop, not both".to_string());
            }
            Ok(Some(if *stop {
                ControlRequest::StatusFollowStop
            } else if *follow {
                ControlRequest::StatusFollowStart { interval_secs: 5 }
            } else {
                ControlRequest::StatusNow
            }))
        }
        Commands::Pause { info_hash } => Ok(Some(ControlRequest::Pause {
            info_hash_hex: require_info_hash_hex(info_hash.as_deref(), "pause")?.to_string(),
        })),
        Commands::Resume { info_hash } => Ok(Some(ControlRequest::Resume {
            info_hash_hex: require_info_hash_hex(info_hash.as_deref(), "resume")?.to_string(),
        })),
        Commands::Delete { info_hash } => Ok(Some(ControlRequest::Delete {
            info_hash_hex: require_info_hash_hex(info_hash.as_deref(), "delete")?.to_string(),
        })),
        Commands::Priority {
            info_hash,
            file_index,
            file_path,
            priority,
        } => {
            let info_hash_hex = require_info_hash_hex(info_hash.as_deref(), "priority")?;
            let target = if let Some(file_index) = file_index {
                ControlPriorityTarget::FileIndex(*file_index)
            } else if let Some(file_path) = file_path {
                ControlPriorityTarget::FilePath(file_path.clone())
            } else {
                return Err("Priority requires either --file-index or --file-path".to_string());
            };

            Ok(Some(ControlRequest::SetFilePriority {
                info_hash_hex: info_hash_hex.to_string(),
                target,
                priority: (*priority).into(),
            }))
        }
        Commands::Add { .. } | Commands::StopClient | Commands::Journal => Ok(None),
    }
}

fn require_info_hash_hex<'a>(
    value: Option<&'a str>,
    command_name: &str,
) -> Result<&'a str, String> {
    value.ok_or_else(|| {
        format!(
            "Missing INFO_HASH_HEX for `superseedr {}`. Get it from `superseedr status` and use the `info_hash_hex` field. Example: `superseedr {} 7f3a9c2d4e1b8a6f0d5c3b2a1908e7d6c5b4a321`",
            command_name, command_name
        )
    })
}

pub fn write_control_command(request: &ControlRequest, watch_path: &Path) -> io::Result<PathBuf> {
    write_control_request(request, watch_path)
}

pub fn wait_for_status_json_after(
    previous_modified_at: Option<SystemTime>,
    timeout: Duration,
) -> io::Result<String> {
    let status_path = status_file_path()?;
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if let Ok(metadata) = fs::metadata(&status_path) {
            let modified_at = metadata.modified().ok();
            let is_new_enough = match (previous_modified_at, modified_at) {
                (Some(previous), Some(current)) => current > previous,
                (None, Some(_)) => true,
                (_, None) => false,
            };

            if is_new_enough || previous_modified_at.is_none() {
                return fs::read_to_string(&status_path);
            }
        }

        if std::time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for a fresh status dump",
            ));
        }

        thread::sleep(Duration::from_millis(200));
    }
}

pub fn status_file_modified_at() -> io::Result<Option<SystemTime>> {
    let status_path = status_file_path()?;
    match fs::metadata(status_path) {
        Ok(metadata) => Ok(metadata.modified().ok()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    // Helper to setup a temp directory if tempfile crate is missing
    fn setup_temp_dir() -> (PathBuf, impl Drop) {
        let dir = std::env::temp_dir().join(format!("superseedr_test_{}", rand::random::<u32>()));
        fs::create_dir_all(&dir).unwrap();
        let dir_clone = dir.clone();
        // Return a dropper to clean up
        struct Cleaner(PathBuf);
        impl Drop for Cleaner {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        (dir, Cleaner(dir_clone))
    }

    #[test]
    fn test_process_input_magnet() {
        let (watch_dir, _cleaner) = setup_temp_dir();
        let magnet_link = "magnet:?xt=urn:btih:5b63529350414441534441534441534441534441";

        write_input_command(magnet_link, &watch_dir).expect("write magnet command");

        // Calculate expected hash
        let hash_bytes = Sha1::digest(magnet_link.as_bytes());
        let expected_name = format!("{}.magnet", hex::encode(hash_bytes));
        let expected_path = watch_dir.join(expected_name);

        assert!(expected_path.exists(), "Magnet file should exist");
        let content = fs::read_to_string(expected_path).unwrap();
        assert_eq!(
            content, magnet_link,
            "File content should be the magnet link"
        );
    }

    #[test]
    fn test_process_input_torrent_path() {
        let (watch_dir, _cleaner) = setup_temp_dir();

        // 1. Create a dummy torrent file to "add"
        let torrent_source_name = "test_linux.torrent";
        let torrent_source_path = watch_dir.join(torrent_source_name);
        {
            let mut f = File::create(&torrent_source_path).unwrap();
            f.write_all(b"dummy torrent content").unwrap();
        }
        let abs_source_path = fs::canonicalize(&torrent_source_path).unwrap();

        // 2. Process the path input
        write_input_command(abs_source_path.to_str().unwrap(), &watch_dir)
            .expect("write path command");

        // 3. Verify the .path file was created
        // The filename is the hash of the *path string*
        let hash_bytes = Sha1::digest(abs_source_path.to_string_lossy().as_bytes());
        let expected_name = format!("{}.path", hex::encode(hash_bytes));
        let expected_path_file = watch_dir.join(expected_name);

        assert!(expected_path_file.exists(), ".path file should be created");

        // 4. Verify content matches the source path
        let content = fs::read_to_string(expected_path_file).unwrap();
        assert_eq!(
            content,
            abs_source_path.to_string_lossy(),
            ".path file should contain the absolute path"
        );
    }

    #[test]
    fn test_process_invalid_path() {
        let (watch_dir, _cleaner) = setup_temp_dir();
        // Pass a non-existent path
        let bad_path = "/path/to/nonexistent/file.torrent";

        // Should not panic
        assert!(write_input_command(bad_path, &watch_dir).is_err());

        // Verify directory is empty (no .path file created)
        let count = fs::read_dir(&watch_dir).unwrap().count();
        assert_eq!(count, 0, "No files should be created for invalid input");
    }

    #[test]
    fn status_command_maps_to_runtime_requests() {
        let follow = Commands::Status {
            follow: true,
            stop: false,
        };
        let request = command_to_control_request(&follow)
            .expect("map status command")
            .expect("control request");
        assert_eq!(
            request,
            ControlRequest::StatusFollowStart { interval_secs: 5 }
        );
    }

    #[test]
    fn priority_requires_one_target() {
        let command = Commands::Priority {
            info_hash: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            file_index: None,
            file_path: None,
            priority: CliPriority::High,
        };
        assert!(command_to_control_request(&command).is_err());
    }

    #[test]
    fn journal_command_is_not_mapped_to_control_request() {
        assert!(matches!(
            command_to_control_request(&Commands::Journal),
            Ok(None)
        ));
    }

    #[test]
    fn delete_without_info_hash_returns_helpful_error() {
        let error = command_to_control_request(&Commands::Delete { info_hash: None })
            .expect_err("missing hash should fail");
        assert!(error.contains("Missing INFO_HASH_HEX"));
        assert!(error.contains("superseedr status"));
        assert!(error.contains("info_hash_hex"));
    }

    #[test]
    fn priority_without_info_hash_returns_helpful_error() {
        let error = command_to_control_request(&Commands::Priority {
            info_hash: None,
            file_index: Some(2),
            file_path: None,
            priority: CliPriority::High,
        })
        .expect_err("missing hash should fail");
        assert!(error.contains("Missing INFO_HASH_HEX"));
        assert!(error.contains("superseedr priority"));
    }
}
