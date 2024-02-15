FROM rustlang/rust:nightly-slim

# install mingw gcc toolchain
RUN apt-get -y -qq update && \
  apt-get -y -qq install gcc-mingw-w64-x86-64 musl-tools

# install rust targets
RUN rustup target add x86_64-pc-windows-gnu
RUN rustup target add x86_64-unknown-linux-musl

# copy cargo config for cross-compiling
COPY cargo.toml /usr/local/cargo/config

# clean image of bloat
RUN apt-get clean autoclean
RUN apt-get autoremove --yes
RUN rm -rf /var/lib/{apt,dpkg,cache,log}/
