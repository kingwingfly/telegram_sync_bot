# About

This is a telegram bot that can be used to download files forwarded to the bot by the owner.

Build with Rust and Teloxide.

The bot will download the file and save it to the specified directory.

Then it will send files saved back to the owner.

The owner can then react to the messages with emoji to manage the file:
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

You need to apply for a telegram api id and hash from [Telegram](https://core.telegram.org/api/obtaining_api_id) first.
(If you always get `Error` during applying, try `cloudflare warp` as VPN)

We provide two ways here:
- native
- pod

Both ways need to run local server in container.
(You can also run local server natively, just omit `-c` and `-i` args when start `fav_sync_bot`.)

Any way, get local server image first:

Prepare (Windows or MacOS only):
```sh
podman machine init -v /path/to/output:/path/to/output bot_machine
podman machine start bot_machine
```

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

### normal way

```sh
podman run --name server -itd --env-file .env --network slirp4netns -p 8081:8081 server

fav_sync_bot -o /path/to/output -l http://localhost:8081 -c podman -i server
```

### run as pod

Build `fav_sync_bot` in to container image:
```sh
# build bot image (you can also download in release page and load it)
podman build --target bot_runner -t bot --network host .

podman pod create sync_bot
podman volume create cache
podman run --pod sync_bot --name server -itd --env-file .env \
    --mount type=volume,source=cache,destination=/app/<TELOXIDE_TOKEN> server
podman run --pod sync_bot --name bot -itd --env-file .env --stop-signal SIGINT\
    -v /path/to/output:/app/output  \
    --mount type=volume,source=cache,destination=/app/<TELOXIDE_TOKEN> bot \
    -l http://server:8081
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
or with local server container and native fav_sync_bot (after the first setup):
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
ExecStop=/bin/bash -c 'kill -SIGINT $MAINPID; for i in {1..5}; do sleep 1; kill -0 $MAINPID 2>/dev/null || exit 0; done; kill -SIGKILL $MAINPID'
ExecStopPost=/usr/bin/podman stop server
Restart=on-failure
Environment="TELOXIDE_TOKEN=<...>"
Environment="OWNER_ID=<...>"

[Install]
WantedBy=multi-user.target
```
or with pod (after the first setup):
```ini
# fav-sync-bot.service
[Unit]
Description=Fav sync bot
After=network-online.target

[Service]
Type=simple
User=<...>
ExecStart=/usr/bin/podman restart sync_bot
ExecStop=/usr/bin/podman stop sync_bot
Restart=on-failure

[Install]
WantedBy=multi-user.target
```
