FROM --platform=linux/amd64 rust:1.69.0 AS chef
WORKDIR /ebu
RUN apt-get update && apt-get install -y git cmake clang pkg-config libssl-dev build-essential
RUN git clone https://github.com/TennisBowling/executionbackup.git . && cargo build --profile highperf --target-dir salida

FROM --platform=linux/amd64 debian:bullseye-slim AS runtime
WORKDIR /ebu
COPY --from=chef /ebu/salida/highperf/executionbackup-rust /usr/local/bin/executionbackup
RUN chmod +x /usr/local/bin/executionbackup
ENTRYPOINT ["/scripts/init.sh"]