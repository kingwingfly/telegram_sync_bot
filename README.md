# About

This is a telegram bot that can be used to download files forwarded to the bot by the owner.

Build with Rust and Teloxide.

## file sent directly to the bot

The bot will download the file and save it to the specified directory.

Then it will send files saved back to the owner (This is necessary for it's prevented to midify user sent messages).

The owner can then react to the returned messages with emoji to manage the file:
- "👍" | "❤": move the file to favorite directory
- "👎": move the file to trash

## file sent to bot managed channel

Initially, the bot owner send `/troggle <bypasskey>` to the bot to troggle among states:
- `paused`: pause the bot
- `active`: sync files and answer reactions
- `partially active`: answer reactions but not sync files

(the `<bypasskey>` can be seen in the log, and send `/bypasskey` to reprint the pwd in the log)

The bot will set "🫡" reaction to the file message to indicate the file is downloading.

Once done, the bot will set "👌". ("😭" if failed, "😨" if canceled)

People can react to the file with emoji, and the bot will count the score of the file.

| Emoji | Score |
| --- | --- |
|👍😁🙏😇🤗|+1|
|❤🔥🥰🎉🍌💋💘😘|+2|
|❤‍🔥|+3|
|👎🤯😱😢🥴🌚😐🖕😨|-1|
|🤬🤮💩🤡💔😡|-2|

If the score >= fav_score_limit, the bot will hard-link the file to favorite directory and pin it.

If the score < delete_score_limit, the bot will hard-link the file to trash and delete from channel.

Otherwise, the bot will hard-link the file to normal directory and unpin the file if necessary.

Note: it takes minites to get ReactionCountUpdate, so the bot will not handle reaction from channel immediately.

# Deploy

You could create a `.env` file with the following content:

```
# Get from botfather
TELOXIDE_TOKEN=xxxxxxxxxx:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
# Your telegram id found in your profile
BYPASS_USERS=xxxxx,xxxxx
# if you want to use local server:
TELEGRAM_API_ID=...
TELEGRAM_API_HASH=...
```

Deploy:
## File size limit 20MB

```
A telegram bot to sync files to local server.

Usage: fav_sync_bot [OPTIONS]

Options:
  -o, --output <OUTPUT>
          The directory to store the files [default: .]
  -l, --local-server-url <LOCAL_SERVER_URL>
          The url if you are using a local server
  -c, --container-manager <CONTAINER_MANAGER>
          The container manager to use if deploying server in a container
  -i, --container-id <CONTAINER_ID>
          The container id or name if deploying server in a container
  -f, --fav-score-limit <FAV_SCORE_LIMIT>
          If score >= limit, fav a file, limit >= 0 (channel only) [default: 10]
  -d, --delete-score-limit <DELETE_SCORE_LIMIT>
          If score < limit, delete a file, limit <= 0 (channel only, e.g `-d-10`) [default: -10]
  -h, --help
          Print help
  -V, --version
          Print version
```

```sh
fav_sync_bot -o /path/to/output
```

## No file size limit (local server)

You need to apply for telegram api id and hash from [Telegram](https://core.telegram.org/api/obtaining_api_id) first.
(If you always get `Error` during applying, try `cloudflare warp` as VPN)

All methods below running local server in container first.

(You can also run local server natively, just omit `-c` and `-i` args when start `fav_sync_bot`.
I'll just skip this method here)

Get local server image first:

Prepare (Windows and MacOS with podman only):
```sh
podman machine init -v /path/to/output:/path/to/output bot_machine
podman machine start bot_machine
```

You can use the following command to build the telegram api bot local server image:
```sh
podman build --target server -t server --network host server
```
Or download and load from the release page (`server.tar.gz`), I've built one through GitHub Action for you.

We provide three ways here:
- native
- pod
- podman kube play

### normal way: server in container but bot native

```sh
podman run --name server -itd --env-file .env -p 8081:8081 server

fav_sync_bot -o /path/to/output -l http://127.0.0.1:8081 -c podman -i server
```

### run as pod

Build `fav_sync_bot` into container image:
```sh
# build bot image
podman build --target bot -t bot --network host bot
```
Or download and load from the release page (bot.tar.gz).

Start server and bot in a pod:
```sh
podman pod create sync_bot
podman volume create cache
podman run --pod sync_bot --name server -itd --env-file .env \
    -v cache:/app/data server
podman run --pod sync_bot --name bot -itd --env-file .env --stop-signal SIGINT\
    -v /path/to/output:/app/output  \
    --mount cache:/app/data bot \
    -l http://server:8081
```

### run with podman kube play

Modify `sync-bot.yaml` to fit your need.

You can download and load `server.tar.gz` and `bot.tar.gz` from the release page first.
Or command below will automatically build the images for you which cost a lot of time.
```sh
podman kube play sync-bot.yaml
```

# Systemd Service
## Native without local server:
```ini
# /etc/systemd/system/sync-bot.service
[Unit]
Description=Telegram file sync bot
After=network-online.target

[Service]
Type=simple
User=<...>
WorkingDirectory=</path/to/output>
ExecStart=/usr/local/bin/fav_sync_bot
Restart=on-failure
Environment="TELOXIDE_TOKEN=<...>"
Environment="BYPASS_USERS=<...>"

[Install]
WantedBy=multi-user.target
```
## or with local server container and native fav_sync_bot (after the first setup):
```ini
# /etc/systemd/system/sync-bot.service
[Unit]
Description=Telegram file sync bot
After=network-online.target

[Service]
Type=simple
User=<...>
WorkingDirectory=</path/to/output>
ExecStartPre=/usr/bin/podman restart server
ExecStart=/usr/local/bin/fav_sync_bot -l http://127.0.0.1:8081 -c podman -i server
ExecStop=/bin/bash -c 'kill -SIGINT $MAINPID; for i in {1..5}; do sleep 1; kill -0 $MAINPID 2>/dev/null || exit 0; done; kill -SIGKILL $MAINPID'
ExecStopPost=/usr/bin/podman stop server
Restart=on-failure
Environment="TELOXIDE_TOKEN=<...>"
Environment="BYPASS_USERS=<...>"

[Install]
WantedBy=multi-user.target
```
## or with pure pod (after images are built or loaded):
```ini
# /etc/container/systemd/users/<UserID>/sync-bot.kube
[Unit]
Description=Telegram file sync bot
After=network-online.target run-media-louis-Local\x20Disk.mount

[Kube]
Yaml=/etc/containers/systemd/users/<UserID>/sync-bot.yaml

[Install]
WantedBy=default.target
```
Search `podman quadlet` for using podman kube play as systemd service.

```sh
systemctl --user daemon-reload
systemctl start --user sync-bot
```

Note: you can use `/usr/lib/systemd/system-generators/podman-system-generator --user --dryrun` to check the generated service file.

# Development

**Rust 2024 is essencial**

Set `DATABASE_URL` in `.env` to generate entity crate.

```
# .env
DATABASE_URL=sqlite://output/data.db
```

Then you can run the following command to create the database and generate the entity:

```sh
cargo install sea-orm-cli
mkdir output
sea-orm-cli migrate refresh
sea-orm-cli generate entity --expanded-format -o bot/src/storage/entity/inner
```
