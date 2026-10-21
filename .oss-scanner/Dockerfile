# Build image for Anthropic's OSS Scanner (https://red.anthropic.com/oss-scanner/).
#
# The scanner builds this file with the repository as the build context and network access, then scans the
# finished image with no network at all. Everything the build and the tests need is therefore fetched here:
# the Rust toolchain, every crate, and the external tools the test suites call. The threat model the scanner
# reads is .oss-scanner/threat_model.md.
#
# Ubuntu 26.04, not Debian, because its `coq` package is Coq 8.20.1: the version the `rocq_typecheck` coqc
# round-trip gate asserts (.github/workflows/rocq-typecheck.yml). Without `coqc` that gate skips its compile step.
FROM ubuntu:26.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config xz-utils coq \
    && rm -rf /var/lib/apt/lists/* \
    && coqc --version

# Rust comes from rustup. The toolchain itself is installed below, from rust-toolchain.toml.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain none

# The `infs run` integration tests execute the modules they build with wasmtime, and abort when it is missing
# under CI. The checksum is the one GitHub publishes for this release asset.
ARG WASMTIME_VERSION=v49.0.4
ARG WASMTIME_SHA256=6561abd54bdf941b6519cce883e0857cc9bce94d92c65287ed838d895a27f5a9
RUN curl -fsSL -o /tmp/wasmtime.tar.xz \
      "https://github.com/bytecodealliance/wasmtime/releases/download/${WASMTIME_VERSION}/wasmtime-${WASMTIME_VERSION}-x86_64-linux.tar.xz" \
    && echo "${WASMTIME_SHA256}  /tmp/wasmtime.tar.xz" | sha256sum -c - \
    && tar -xJf /tmp/wasmtime.tar.xz -C /opt \
    && ln -s "/opt/wasmtime-${WASMTIME_VERSION}-x86_64-linux/wasmtime" /usr/local/bin/wasmtime \
    && rm /tmp/wasmtime.tar.xz \
    && wasmtime --version

# The cross-compiler tests (`algo_iter`, `algo_array`) compile the same algorithms with rustc and Zig and assert
# that all three Wasm modules agree, input by input. Without both compilers they skip and pass without comparing.
# The checksum is the one ziglang.org publishes in its download index.
ARG ZIG_VERSION=0.17.0
ARG ZIG_SHA256=1cbe9df9f27e6b78d14ccbca43b6703a404ef79ef1c463de901d7f088d4e2026
RUN curl -fsSL -o /tmp/zig.tar.xz \
      "https://ziglang.org/download/${ZIG_VERSION}/zig-x86_64-linux-${ZIG_VERSION}.tar.xz" \
    && echo "${ZIG_SHA256}  /tmp/zig.tar.xz" | sha256sum -c - \
    && tar -xJf /tmp/zig.tar.xz -C /opt \
    && ln -s "/opt/zig-x86_64-linux-${ZIG_VERSION}/zig" /usr/local/bin/zig \
    && rm /tmp/zig.tar.xz \
    && zig version

# The scanner's contract: the checkout lives inside the image, at /src.
COPY . /src
WORKDIR /src

# Install the toolchain rust-toolchain.toml names, with the wasm32 target the cross-compiler tests build their
# Rust reference with, then fetch every crate the workspace, its tests and its benches need. Cargo.lock is not
# tracked, so `cargo fetch` resolves one here and every later step uses it.
RUN rustup toolchain install \
    && rustup target add wasm32-unknown-unknown \
    && rustc --version \
    && cargo fetch

# Nothing after this point may reach the network, here or in the scanner.
ENV CARGO_NET_OFFLINE=true

# Build as CI does (the dev profile), but the whole workspace: the default members leave out tools/, which
# holds inf-wasmparser, the fork of wasmparser the linker reads external modules with. `--no-run` compiles
# every test binary up front, so a test run in the scanner starts without a long compile.
RUN cargo build --workspace \
    && cargo test --workspace --no-run

# Run the default members' tests, as CI does, so the log shows the image works. A failure is reported but does
# not fail the build: the scanner can still use an image with one broken test. `--no-fail-fast` keeps one
# failing test binary from hiding the results of the ones after it.
RUN cargo test --no-fail-fast || echo "WARNING: cargo test reported failures"
