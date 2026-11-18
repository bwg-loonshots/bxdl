#!/bin/sh
# Run on the Ubuntu amd64 builder from the reviewed BXDL checkout.
set -eu
revision=$(git rev-parse HEAD)
source_dir=$(git rev-parse --show-toplevel)
output="$source_dir/bin"
mkdir -p "$output"

# Official rust:1.86.0-bullseye Linux amd64 manifest. Link against glibc 2.31
# so the resulting CLI can satisfy the package's glibc 2.34 ceiling.
image=docker.io/library/rust@sha256:1110399f568f1dbe838e58f15b4162d899cb95f450f5f0ffa739614f3a4c32f1
docker run --rm --platform linux/amd64 --user "$(id -u):$(id -g)" \
    --mount "type=bind,src=$source_dir,dst=/src,readonly" \
    --mount "type=bind,src=$output,dst=/out" --workdir /src \
    --env "BXDL_REVISION=$revision" --env HOME=/tmp/bxdl-home \
    --env CARGO_HOME=/tmp/bxdl-cargo --env CARGO_TARGET_DIR=/tmp/bxdl-target \
    --env RUSTUP_TOOLCHAIN=1.86.0 "$image" /bin/sh -eu -c '
        mkdir -p "$HOME"
        cargo build --locked --release
        install -m 0755 "$CARGO_TARGET_DIR/release/bxdl" /out/bxdl
    '
