#![windows_subsystem = "windows"]
mod tools;
mod core;
mod app;
mod pages;
mod cli;

use eframe::{run_native, NativeOptions};
use std::process::exit;
use crate::cli::*;

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
        multisampling: 0,
        ..Default::default()
    };

    run_native(
        "Pavlov Replay Toolbox",
        native_options,
        Box::new(|cc| Ok(Box::new(app::ReplayApp::new(cc)))),
    )
}

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
