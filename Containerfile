# Loglem in un container.
#
#   podman build -t loglem -f Containerfile .
#   podman run -d --name loglem -p 127.0.0.1:8790:8790 \
#     -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='una password lunga' \
#     -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro \
#     loglem
#
# Poi apri http://127.0.0.1:8790 e aggiungi la cartella /logs/myapp come sorgente.
# Podman, Docker e il journal dell'host non sono raggiungibili da dentro: per quelli usa Loglem
# sull'host, oppure monta i file di log in /logs.

# Compilato su Debian 11 (glibc 2.31): il programma funziona anche su sistemi più vecchi.
FROM docker.io/library/rust:1-bullseye AS build
ARG CARGO_FLAGS=""
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
# Prima solo le dipendenze: se cambia il codice, non si ricompilano.
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs \
    && cargo build --release --locked ${CARGO_FLAGS} && rm -rf src
COPY src src
COPY web web
RUN touch src/main.rs src/lib.rs \
    && cargo build --release --locked ${CARGO_FLAGS} \
    && ./target/release/loglem self-check

FROM docker.io/library/debian:bookworm-slim
RUN useradd --create-home --uid 10001 loglem \
    && mkdir /data /logs \
    && chown loglem /data
COPY --from=build /src/target/release/loglem /usr/local/bin/loglem
COPY container/entrypoint.sh /usr/local/bin/loglem-entrypoint
USER loglem
ENV LOGLEM_DATA_DIR=/data
VOLUME /data
EXPOSE 8790
HEALTHCHECK --interval=1m --timeout=20s CMD loglem check --minutes 30 || exit 1
ENTRYPOINT ["loglem-entrypoint"]
