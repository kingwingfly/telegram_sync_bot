FROM docker.io/rustlang/rust:nightly-alpine AS bot_builder

RUN apk update \
    && apk add --no-cache --purge libc-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /work
COPY . .

RUN cargo fetch
RUN cargo build --release

########################################

FROM docker.io/alpine:latest AS server_builder

RUN apk update \
    && apk add --no-cache --purge alpine-sdk linux-headers git zlib-dev openssl-dev gperf cmake

RUN git clone --recursive https://github.com/tdlib/telegram-bot-api.git \
    && cd telegram-bot-api \
    && rm -rf build \
    && mkdir build \
    && cd build \
    && cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX:PATH=.. .. \
    && cmake --build . --target install \
    && cd ../.. \
    && ls -l telegram-bot-api/bin/telegram-bot-api*

########################################

FROM docker.io/alpine:latest AS bot_runner

WORKDIR /app

COPY --from=bot_builder /work/target/release/fav_sync_bot /app/

CMD ["/app/fav_sync_bot", "/app/output"]

########################################

FROM docker.io/alpine:latest AS server_runner

RUN apk update \
    && apk add --no-cache --purge bash openssl zlib libstdc++

WORKDIR /app

COPY --from=server_builder /telegram-bot-api/bin/telegram-bot-api /app/

EXPOSE 8081

CMD ["/app/telegram-bot-api", "--local"]
