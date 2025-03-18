# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

<!--
Note: In this file, do not use the hard wrap in the middle of a sentence for compatibility with GitHub comment style markdown rendering.
-->

## [Unreleased]
## [0.5.2] - 2025-03-14

- fix bug: duplicate extension in file_name

## [0.5.1] - 2025-03-14

- support audio
- better file_name extraction

## [0.5.0] - 2025-03-14

- support kubernetes
- the unique volume of host to container in server and bot
- try hard-linking when move file from local server to bot-output in local-server mode
- use `data` instead of `output` as the argument name
- `-f` for favorite, `-F` for dislike

## [0.4.0] - 2025-03-12

- move from `sqlx` to `sea-orm`
- improve with a download manager
- play with `CancellationToken`, better code structure
- better file-state and transport-state management
- improve with foreign key
- sub command to delete file/msg from fs, db and telegram
- split group msgs
- TryMultipleTimers trait to lift success possibility

## [0.3.2] - 2025-03-10

- command `/trogglesync` to stop saving new files and only works as a reaction handler
- sql improvements
- fix bug: do not need unpin deleted message

## [0.3.1] - 2025-03-10

- fix bug: the bot will check the path exists before operate
- fix bug: delete the message out of control
- log more detailed Context
- pin while fav, unpin while unfav or delete
- improve direct to bot msg experience

## [0.3.0] - 2025-03-10

- support channel management: the bot will generate a dynamic bypass password, use `/unpause <password>` to unpause the bot
- rename env var `OWNER_ID` to `BYPASS_USERS`
- do not remove the file, move to trash instead
- move from `sled` to `sqlite`

## [0.2.5] - 2025-03-09

- speed up build with `ninja`
- the bot image will stop with SIGINT
- the server image will wait 5 seconds before exit

## [0.2.4] - 2025-03-07

- kube play support
- cautions: user updated to this version should reload or rebuild the images

## [0.2.3] - 2025-03-07

- release images
- improve document
- improve Containerfile

## [0.2.2] - 2025-03-07

- fix bug in replying

## [0.2.1] - 2025-03-07

- log when download start
- fix bug: now loop GetFile

## [0.2.0] - 2025-03-07

- local server support

## [0.1.5] - 2025-03-06

- document download support

## [0.1.4] - 2025-03-06

- async download

## [0.1.3] - 2025-03-06

- use emoji to manage files

## [0.1.2] - 2025-03-05

- reply after downloading

## [0.1.1] - 2025-03-05

- sha256 file name
- systemd service example

## [0.1.0] - 2025-03-05

- MVP
