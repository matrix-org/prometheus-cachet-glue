FROM docker.io/alpine:edge as builder
COPY . /src
RUN apk add --no-cache \
      cargo \
      build-base \
      openssl-dev \
 && cd /src \
 && cargo build --release


FROM docker.io/matrixdotorg/base-alpine
COPY --from=builder /src/target/release/prometheus-cachet-glue /usr/local/bin/prometheus-cachet-glue
RUN apk add --no-cache \
      libssl1.0 \
      libgcc \
      ca-certificates
COPY docker/root /
