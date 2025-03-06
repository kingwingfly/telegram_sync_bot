# About

This is a telegram bot that can be used to download files forwarded to the bot by the owner.

Build with Rust and Teloxide.

The bot will download the file and save it to the specified directory.

Then it will send the saved file to the owner.

The owner can react to the message with emoji to manage the file:
- "👍" | "❤": move the file to favorite directory
- "👎": delete the file

# Usage

You need create a `.env` file with the following content:

```sh
# Get from botfather
TELOXIDE_TOKEN=xxxxxxxxxx:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
# Your telegram id found in your profile
OWNER_ID=xxxxxxxxxx
# if you want to use local server:
TELEGRAM_API_ID=...
TELEGRAM_API_HASH=...
```

Deploy:
## File size limit 20MB

```sh
fav_sync_bot -o /path/to/output
```

## No file size limit (local server)

You need apply for a telegram api id and hash from [Telegram](https://core.telegram.org/api/obtaining_api_id) first.
(If you always get `Error` during applying, try `cloudflare warp` as VPN)

You can use the following command to build the telegram api bot local server image:
```sh
podman build --target server_runner -t server --network host .
```
Or download and load one
```sh
cd /tmp
curl -LO https://github.com/kingwingfly/fav_sync_bot/releases/download/v0.2.0/server.tar.gz
podman load -i server.tar.gz
```

```sh
# Windows or MacOS only
podman machine init -v /path/to/output:/path/to/output bot_machine

podman run --name server -itd --env-file .env --network slirp4netns -p 8081:8081 server

fav_sync_bot -o /path/to/output -l http://localhost:8081 -c podman -i server
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
or with local server:
```ini
# fav-sync-bot.service
[Unit]
Description=Fav sync bot
After=network-online.target

[Service]
Type=simple
User=<...>
WorkingDirectory=</path/to/output>
ExecStartPre=/usr/bin/podman restart server
ExecStart=/usr/local/bin/fav_sync_bot -l http://localhost:8081 -c podman -i server
Restart=on-failure
Environment="TELOXIDE_TOKEN=<...>"
Environment="OWNER_ID=<...>"

[Install]
WantedBy=multi-user.target
```
