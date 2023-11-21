FROM --platform=linux/amd64 rust:1.69.0 AS chef
ARG RELEASE_TAG
WORKDIR /ebu
RUN apt-get update && apt-get install -y git cmake clang pkg-config libssl-dev build-essential
RUN git clone https://github.com/TennisBowling/executionbackup.git .
RUN if [[ "$RELEASE_TAG" ]] ; then git checkout $RELEASE_TAG ; fi
RUN cargo install --path .

FROM --platform=linux/amd64 debian:bullseye-slim AS runtime
COPY --from=chef /usr/local/cargo/bin/executionbackup /usr/local/bin/executionbackup
RUN chmod +x /usr/local/bin/executionbackup
ENTRYPOINT ["/scripts/init.sh"]