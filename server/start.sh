#!/bin/bash

if [ -z "$1" ]; then
    echo "Usage: $0 <data dir>" >&2
    exit 1
fi

# run telegram-bot-api in background
/app/telegram-bot-api -d $1 --local &
app_pid=$!

# check if telegram-bot-api is running
if [ ! -d /proc/$app_pid ]; then
    echo "Error: Failed to start telegram-bot-api" >&2
    exit 1
fi

# function to stop telegram-bot-api
function on_sigterm {
    if [ -z "$app_pid" ] || ! kill -0 "$app_pid" 2>/dev/null; then
        echo "No valid app_pid to stop" >&2
        exit 1
    fi
    echo "SIGTERM received. Waiting for shutdown delay..."
    sleep "${SHUTDOWN_DELAY:-5}"
    echo "Stopping the app now..."
    kill -TERM "$app_pid"
    wait "$app_pid"
    exit 0
}

# trap signals to stop telegram-bot-api
trap 'on_sigterm' SIGTERM SIGINT SIGHUP

# wait for telegram-bot-api to exit and return its exit code
wait "$app_pid"
exit_code=$?
if [ $exit_code -ne 0 ]; then
    echo "telegram-bot-api exited with code $exit_code" >&2
    exit $exit_code
fi
