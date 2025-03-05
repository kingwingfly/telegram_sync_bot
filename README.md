# About

This is a telegram bot that can be used to download files forwarded to the bot by the owner.

Build with Rust and Teloxide.

# Usage

You need create a `.env` file with the following content:

```sh
# Get from botfather
TELOXIDE_TOKEN=xxxxxxxxxx:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
# Your telegram id found in your profile
OWNER_ID=xxxxxxxxxx
```

Deploy:
## Native (File size limit 20MB)

```sh
fav_sync_bot /path/to/output
```

## With docker (No file size limit) \[WIP\]

WIP: I'm stuck since I cannot get the app\_id and app\_hash from telegram.

But you can still run the bot with docker although it's unnecessary.

```sh
# WIP podman build --target server_runner -t server --network host .
podman build --target bot_runner -t bot --network host .
```

```sh
# Windows or MacOS only
podman machine init -v /path/to/output:/path/to/output bot_machine

# WIP podman run  -itd -e TELEGRAM_API_ID=<api_id> -e TELEGRAM_API_HASH=<api_hash> server
podman run -itd --env-file .env -v /path/to/output:/app/output --stop-signal SIGINT bot
```

# Systemd Service
```ini
# fav-sync-bot.service
[Unit]
Description=Fav sync bot
After=network-online.target

[Service]
Type=simple
User=<...>
WorkingDirectory=</path/to/output>
ExecStart=/usr/local/bin/fav_sync_bot
Restart=on-failure
Environment="TELOXIDE_TOKEN=<...>"
Environment="OWNER_ID=<...>"

[Install]
WantedBy=multi-user.target
```
