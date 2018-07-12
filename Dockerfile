FROM docker.io/alpine:3.8 as builder
COPY . /src
RUN apk add --no-cache \
      cargo \
      build-base \
      openssl-dev \
 && cd /src \
 && cargo build --release


FROM docker.io/alpine:3.8
ENV UID=1337 \
    GID=1337
COPY --from=builder /src/target/release/prometheus-cachet-glue /usr/local/bin/prometheus-cachet-glue
RUN apk add --no-cache \
      libssl1.0 \
      libgcc \
      ca-certificates \
      s6 \
      su-exec
COPY docker/root /
CMD ["/bin/s6-svscan", "/etc/s6.d/"]
