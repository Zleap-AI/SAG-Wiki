# Build with: docker build --platform linux/amd64 -f scripts/ci/Dockerfile scripts/ci
ARG RUST_BASE=rust:1.99.1-bookworm@sha256:edaa2a8cc254d5936207af267114814c61cceea40b46eaa4e976068af55e20e7
FROM ${RUST_BASE}
ENV CI_PROJECT_DIR=/opt/sag-ci \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_TOOLCHAIN=1.99.1 \
    RUSTUP_DIST_SERVER=https://rsproxy.cn
WORKDIR /opt/sag-ci
COPY setup.sh /opt/sag-ci/setup.sh
RUN bash -c 'source ./setup.sh build' \
    && cp /usr/local/cargo/bin/cargo-zigbuild /usr/local/bin/cargo-zigbuild \
    && rm -rf /usr/local/cargo/registry /usr/local/cargo/git /root/.cache \
    && rm -f /opt/sag-ci/.ci-tools/node.tar.xz \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
ENV SAG_FPK_TOOLCHAIN_IMAGE=1
ENV PATH=/opt/sag-ci/.ci-tools/node-v24.13.0-linux-x64/bin:/opt/sag-ci/.ci-tools/python/bin:${PATH} \
    FNPACK=/opt/sag-ci/tools/fnpack
WORKDIR /builds
