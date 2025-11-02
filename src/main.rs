#![windows_subsystem = "windows"]
mod tools;
mod app;
mod pages;

use std::fs;
use std::path::Path;
use std::process::exit;

use eframe::{run_native, NativeOptions};
use reqwest::blocking::Client;

use crate::tools::replay_processor::MetaData;
use crate::tools::replay_processor::download_replay;
use crate::tools::replay_processor::API_BASE_URL;

pub struct CliArg {
    key: &'static str,
    flag: bool,
    description: &'static str
}

pub const CLI_ARG_REPLAY : CliArg = CliArg {
    key: "-r",
    flag: false,
    description: "Replay ID. Giving this argument bypasses graphical UI."
};
pub const CLI_ARG_ALTERNATE_NAME : CliArg = CliArg {
    key: "--alt",
    flag: true,
    description: "Alternate naming schema puts timestamp first. (file browsers can easily sort timeline by name)."
};
pub const CLI_ARG_ISO8601 : CliArg = CliArg {
    key: "--iso8601",
    flag: true,
    description: "(NOT SUPPORTED BY NTFS/WINDOWS!) Sets timestamp in ISO8601 format."
};
pub const CLI_ARG_UTC : CliArg = CliArg {
    key: "--utc",
    flag: true,
    description: "Timestamp is in UTC timezone."
};
pub const CLI_ARG_OUTPUT : CliArg = CliArg {
    key: "-o",
    flag: false,
    description: "Output name. Used only with '-r' -option."
};
pub const CLI_ARG_HELP : CliArg = CliArg {
    key: "-h",
    flag: true,
    description: "Print help."
};

pub const CLI_ARGS : [CliArg; 6] = [CLI_ARG_REPLAY, CLI_ARG_OUTPUT, CLI_ARG_ALTERNATE_NAME, CLI_ARG_ISO8601, CLI_ARG_UTC, CLI_ARG_HELP];

pub struct CliCfg {
    alt_name_scheme: bool,
    iso8601: bool,
    utc: bool
}

fn print_help(){
    println!("Command Line Interface (CLI) arguments:");
    println!(" {:14} {:10} {}" ,"KEY", "" ,"DESCRIPTION");
    for arg in CLI_ARGS {
        let mut requires_value= "";
        if !arg.flag {
            requires_value="[VALUE]";
        }
        println!(" {:14} {:10} {}", arg.key, requires_value, arg.description);
    }
    println!("NOTE: CLI arguments has no effect on GUI side.\n");
}

fn find_cli_arg(key: &str) -> Option<CliArg> {
    for arg in CLI_ARGS {
        if key!=arg.key { continue; }
        return Some(arg);
    }
    None
}

fn main_ui() -> eframe::Result<()>{
    let icon_data = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("Failed to load icon")
        .to_rgba8();
    let (icon_width, icon_height) = icon_data.dimensions();

    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_min_inner_size([975.0, 600.0])
            .with_inner_size([975.0, 768.0])
            .with_decorations(true)
            .with_drag_and_drop(true)
            .with_resizable(true)
            .with_title("Pavlov Replay Toolbox")
            .with_icon(egui::IconData {
                rgba: icon_data.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        centered: true,
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 2,
        ..Default::default()
    };

    run_native(
        "Pavlov Replay Toolbox",
        native_options,
        Box::new(|cc| Ok(Box::new(app::ReplayApp::new(cc)))),
    )
}

fn main_cli(replay_id: String, output_path: Option<String>, cfg: CliCfg){

    let replay_id_clone = replay_id.to_string();
    let download_dir = match std::env::current_dir(){
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

        println!("Downloading replay '{}'...", &replay_id);

        let replay_data = match download_replay(&replay_id, None) {
            Ok(data) => data,
            Err(e) => return Err(format!("Failed to download replay data: {}", e).into())
        };

        println!("Downloading metadata.");

        let metadata_result = match client
            .get(&format!("{}/meta/{}", API_BASE_URL, replay_id_clone))
            .send() {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        return Err(format!(
                            "Failed to fetch replay metadata: Server returned {} - {}", 
                            resp.status().as_u16(),
                            resp.status().canonical_reason().unwrap_or("Unknown error")
                        ).into());
                    }
                    
                    match resp.json::<MetaData>() {
                        Ok(data) => {
                            data
                        },
                        Err(e) => return Err(format!(
                            "Failed to parse replay metadata: {}. The API format may have changed.", e
                        ).into())
                    }
                },
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
                let ts = metadata_result.created
                    .parse::<i64>()
                    .map_err(|e| format!("Invalid timestamp format: {}", e))?;
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.fixed_offset())
                    .ok_or_else(|| "Invalid timestamp".into())
            }) {
                Ok(dt) => {
                    dt
                },
                Err(e) => return Err(format!("Failed to parse replay date: {}", e).into())
            };

        let formatted_date = 
        if cfg.iso8601 {
            if cfg.utc {
                created_datetime.to_utc().format("%+")
            }else{
                created_datetime.format("%+")
            }
        }else{
            if cfg.utc {
                created_datetime.to_utc().format("%Y.%m.%d-%H.%M.%S")
            }else{
                created_datetime.format("%Y.%m.%d-%H.%M.%S")
            }
        };

        let replacement_char = if cfg.alt_name_scheme { "_" } else { "-" };
        let sanitized_name = metadata_result.friendly_name.replace([' ','<','>',':','"','/',',','\\','?','*','='], replacement_char);
        let filename = 
        if cfg.alt_name_scheme{
            format!(
                "{} {} {} {}.replay",
                formatted_date,
                metadata_result.game_mode,
                sanitized_name,
                replay_id_clone
            )
        }else{
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
                }else{
                    download_dir.join(name)
                }
            },
            None => download_dir.join(filename)
        };

        println!("Saving to file to '{}'.", output_file.display());

        match fs::write(output_file, replay_data) {
            Ok(_) => {},
            Err(e) => return Err(format!("Failed to save replay file: {}", e).into())
        }

        println!("Replay saved successfully.");

        Ok(())
    })();

    match result {
        Ok(_ok) => {},
        Err(_err) => {
            println!("Error {}",_err);
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
fn ensure_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            let _ = AllocConsole();
        }
    }
}

// Non-Windows platforms do not require special console handling
#[cfg(not(windows))]
fn ensure_console() {}

fn main(){
    let has_cli_args = std::env::args_os().nth(1).is_some();
    if has_cli_args {
        ensure_console();
    }

    // CLI configurations & flags
    let mut cli_replay_id: Option<String> = None;
    let mut cli_filepath: Option<String> = None;
    let mut cli_config: CliCfg = CliCfg {
        alt_name_scheme: false,
        iso8601: false,
        utc: false,
    };

    // Get arguments & flags
    let mut args = std::env::args();
    let _ = args.next();

    // Process arguments & flags
    while let Some(arg) = args.next() {

        match find_cli_arg(&arg) {
            Some(arg) => {

                match arg.key {
                    "-r" =>{
                        if let Some(next) = args.next() {
                            println!("Replay ID set to '{}'",next);
                            cli_replay_id=Some(next);
                        }else {
                            println!("flag {} must have a value!",arg.key);
                            return;
                        }
                    },
                    "-o" =>{
                        if let Some(next) = args.next() {
                            println!("Output filename set to '{}'",next);
                            cli_filepath=Some(next);
                        }else {
                            println!("flag {} must have a value!",arg.key);
                            return;
                        }
                    },
                    "--alt" => {
                        cli_config.alt_name_scheme = true;
                        println!("flag {} => Using alternate naming schema.", arg.key);
                    },
                    "--iso8601" => {
                        cli_config.iso8601 = true;
                        println!("flag {} => Using alternate date format (ISO8601)", arg.key);
                    },
                    "--utc" => {
                        cli_config.utc = true;
                        println!("flag {} => Using UTC timestamps", arg.key);
                    },
                    "-h" =>{
                        print_help();
                        exit(0);
                    },
                    _ => {}
                }
            },
            None => {}
        }
    }

    // Launch in CLI mode if replay id was provided as CLI argument, otherwise in GUI mode
    if let Some(replay_id) = cli_replay_id.clone()  {
        main_cli(replay_id,cli_filepath, cli_config)
    }else{
        match main_ui() {
            Ok(_data) => {},
            Err(_err) => {
                println!("Error {}",_err);
                exit(1);
            }
        } ;
    }

}
