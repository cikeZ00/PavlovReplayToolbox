use std::path::Path;
use std::process::exit;

use reqwest::blocking::Client;

use crate::tools::replay_processor::{
    download_replay_to_path, DownloadOptions, MetaData, API_BASE_URL,
};

pub struct CliArg {
    pub key: &'static str,
    pub flag: bool,
    pub description: &'static str,
}

pub const CLI_ARG_REPLAY: CliArg = CliArg {
    key: "-r",
    flag: false,
    description: "Replay ID. Giving this argument bypasses graphical UI.",
};
pub const CLI_ARG_ALTERNATE_NAME: CliArg = CliArg {
    key: "--alt",
    flag: true,
    description: "Alternate naming schema puts timestamp first. (file browsers can easily sort timeline by name).",
};
pub const CLI_ARG_ISO8601: CliArg = CliArg {
    key: "--iso8601",
    flag: true,
    description: "(NOT SUPPORTED BY NTFS/WINDOWS!) Sets timestamp in ISO8601 format.",
};
pub const CLI_ARG_UTC: CliArg = CliArg {
    key: "--utc",
    flag: true,
    description: "Timestamp is in UTC timezone.",
};
pub const CLI_ARG_OUTPUT: CliArg = CliArg {
    key: "-o",
    flag: false,
    description: "Output name. Used only with '-r' -option.",
};
pub const CLI_ARG_HELP: CliArg = CliArg {
    key: "-h",
    flag: true,
    description: "Print help.",
};

pub const CLI_ARGS: [CliArg; 6] = [
    CLI_ARG_REPLAY,
    CLI_ARG_OUTPUT,
    CLI_ARG_ALTERNATE_NAME,
    CLI_ARG_ISO8601,
    CLI_ARG_UTC,
    CLI_ARG_HELP,
];

pub struct CliCfg {
    pub alt_name_scheme: bool,
    pub iso8601: bool,
    pub utc: bool,
}

pub fn print_help() {
    println!("Command Line Interface (CLI) arguments:");
    println!(" {:14} {:10} {}", "KEY", "", "DESCRIPTION");
    for arg in CLI_ARGS {
        let mut requires_value = "";
        if !arg.flag {
            requires_value = "[VALUE]";
        }
        println!(" {:14} {:10} {}", arg.key, requires_value, arg.description);
    }
    println!("NOTE: CLI arguments has no effect on GUI side.\n");
}

pub fn find_cli_arg(key: &str) -> Option<CliArg> {
    for arg in CLI_ARGS {
        if key != arg.key {
            continue;
        }
        return Some(arg);
    }
    None
}

pub fn main_cli(replay_id: String, output_path: Option<String>, cfg: CliCfg) {
    let replay_id_clone = replay_id.to_string();
    let download_dir = match std::env::current_dir() {
        Ok(wd) => wd,
        Err(_err) => {
            exit(127);
        }
    };

    let client = match Client::builder().build() {
        Ok(client) => client,
        Err(_e) => {
            return;
        }
    };

    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        println!("Downloading metadata.");

        let metadata_result = match client
            .get(&format!("{}/meta/{}", API_BASE_URL, replay_id_clone))
            .send()
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return Err(
                        format!(
                            "Failed to fetch replay metadata: Server returned {} - {}",
                            resp.status().as_u16(),
                            resp.status().canonical_reason().unwrap_or("Unknown error")
                        )
                        .into(),
                    );
                }

                match resp.json::<MetaData>() {
                    Ok(data) => data,
                    Err(e) => return Err(format!(
                        "Failed to parse replay metadata: {}. The API format may have changed.",
                        e
                    )
                    .into()),
                }
            }
            Err(e) => {
                return if e.is_timeout() {
                    Err("Connection timed out while fetching replay metadata.".into())
                } else if e.is_connect() {
                    Err("Failed to connect to metadata server. Please check your internet connection.".into())
                } else {
                    Err(format!("Network error retrieving metadata: {}", e).into())
                }
            }
        };

        println!("Processing metadata.");

        let created_datetime = match chrono::DateTime::parse_from_rfc3339(&metadata_result.created)
            .or_else(|_| -> Result<_, Box<dyn std::error::Error>> {
                let ts = metadata_result
                    .created
                    .parse::<i64>()
                    .map_err(|e| format!("Invalid timestamp format: {}", e))?;
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.fixed_offset())
                    .ok_or_else(|| "Invalid timestamp".into())
            })
        {
            Ok(dt) => dt,
            Err(e) => return Err(format!("Failed to parse replay date: {}", e).into()),
        };

        let formatted_date = if cfg.iso8601 {
            if cfg.utc {
                created_datetime.to_utc().format("%+")
            } else {
                created_datetime.format("%+")
            }
        } else {
            if cfg.utc {
                created_datetime.to_utc().format("%Y.%m.%d-%H.%M.%S")
            } else {
                created_datetime.format("%Y.%m.%d-%H.%M.%S")
            }
        };

        let replacement_char = if cfg.alt_name_scheme { "_" } else { "-" };
        let sanitized_name = metadata_result
            .friendly_name
            .replace([' ', '<', '>', ':', '"', '/', ',', '\\', '?', '*', '='], replacement_char);
        let filename = if cfg.alt_name_scheme {
            format!(
                "{} {} {} {}.replay",
                formatted_date,
                metadata_result.game_mode,
                sanitized_name,
                replay_id_clone
            )
        } else {
            format!(
                "{}-{}-{}({}).replay",
                sanitized_name,
                metadata_result.game_mode,
                formatted_date,
                replay_id_clone
            )
        };

        let output_file = match output_path {
            Some(name) => {
                let path = Path::new(&name);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    download_dir.join(name)
                }
            }
            None => download_dir.join(filename),
        };

        println!("Downloading replay '{}'...", &replay_id);

        let download_options = DownloadOptions {
            use_disk_cache: true,
            cache_dir: download_dir.join(".replay_cache"),
            max_parallel_downloads: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(4),
        };

        download_replay_to_path(
            &replay_id,
            download_options,
            &output_file,
            Some(metadata_result),
            None,
        )
        .map_err(|e| format!("Failed to download replay data: {}", e))?;

        println!("Saved replay to '{}'", output_file.display());

        println!("Replay saved successfully.");

        Ok(())
    })();

    match result {
        Ok(_ok) => {}
        Err(_err) => {
            println!("Error {}", _err);
            exit(1);
        }
    }
}

// When running in CLI mode on Windows, ensure a console is attached to display output
#[cfg(windows)]
const ATTACH_PARENT_PROCESS: u32 = u32::MAX;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(dw_process_id: u32) -> i32;
    fn AllocConsole() -> i32;
}

#[cfg(windows)]
pub fn ensure_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            let _ = AllocConsole();
        }
    }
}

// Non-Windows platforms do not require special console handling
#[cfg(not(windows))]
pub fn ensure_console() {}
