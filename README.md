[![Download Latest Release](https://img.shields.io/badge/Download-Latest%20Release-brightgreen)](https://github.com/cikeZ00/PavlovReplayToolbox/releases/latest)

# PavlovReplayToolbox

PavlovReplayToolbox is a Rust-based toolbox designed to facilitate the download and processing of replay files within the Pavlov VR game. This toolbox provides utilities to download replays from PavlovTV and process them into full .replay files.

## Features

- **Replay Downloading**: Download replays directly from PavlovTV.
- **Replay Processing**: Convert downloaded data into full .replay files for further use.

## How to use
- Go to [![Download](https://img.shields.io/badge/Download-00FF00)](https://github.com/cikeZ00/PavlovReplayToolbox/releases/latest)
- Download `` PavlovReplayToolbox.exe `` and open it.
- Find the replay you want.
- (Optional) Open settings and set download location.
- Click on ``Download & Process``, once it's done your replay should be in the whichever download directory you have set.

## Screenshots
<p align="center">
  <img src="https://github.com/user-attachments/assets/b0dbf9cc-14ab-45d0-88e8-85fd149e7d3f" width="30%" />
  <img src="https://github.com/user-attachments/assets/59af3630-f940-44ae-bc76-af878658bee9" width="30%" />
  <img src="https://github.com/user-attachments/assets/97488e14-d1b7-4601-9c7b-e1e1c4a4c0d9" width="30%" />

## CLI Mode

PavlovReplayToolbox also supports a command-line interface for advanced users and automation.

### Usage

You can run the toolbox in CLI mode by providing arguments when launching the executable. This bypasses the graphical UI.

```sh
PavlovReplayToolbox.exe -r <REPLAY_ID> [options]
```

#### Available Arguments

| Argument      | Description                                                                 |
|---------------|-----------------------------------------------------------------------------|
| `-r [VALUE]`  | Replay ID. Giving this argument bypasses graphical UI.                      |
| `-o [VALUE]`  | Output name. Used only with `-r` option.                                    |
| `--alt`       | Alternate naming schema puts timestamp first for easier sorting.             |
| `--iso8601`   | (NOT SUPPORTED BY NTFS/WINDOWS!) Sets timestamp in ISO8601 format.          |
| `--utc`       | Timestamp is in UTC timezone.                                               |
| `-h`          | Print help.                                                                 |

**Example:**

```sh
PavlovReplayToolbox.exe -r 3097aad10081b37190df7e5fffdaf9bf --alt --utc -o my_replay.replay
```

## Getting Started

These instructions will help you set up the PavlovReplayToolbox on your local machine for development and usage.

### Prerequisites

- Rust (latest stable version)
- Cargo (latest stable version)

## Usage

1. Clone the repository:
    ```sh
    git clone https://github.com/cikeZ00/PavlovReplayToolbox.git
    ```

2. Navigate to the project directory:
    ```sh
    cd PavlovReplayToolbox
    ```

3. Build the project:
    ```sh
    cargo build --release
    ```

4. Run the project:
    ```sh
    cargo run
    ```


