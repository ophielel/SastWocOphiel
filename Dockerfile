FROM ubuntu:22.04 AS builder

LABEL maintainer="OphielSast"
LABEL version="1.0"
LABEL description="OphielSast Project -Woc 2026 Rust Kernal Module Demo Builder"

Run apt-get update && apt-get install -y \
	qemu-system-x86 \
	make gcc clang lld \
	python3 bc bison flex \
	pkg-config cpio gzip git \
	libssl-dev libelf-dev \
	curl \
	&& rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup component add rust-src

WORKDIR /app
COPY . .

RUN make setup
RUN make build

FROM ubuntu:22.04

LABEL maintainer="OphielSast"
LABEL version="1.0"
LABEL descrption="OphielSast Project - Woc 2026 Rust Kernal Module Demo Runtime"

RUN apt-get update && apt-get install -y \
	qemu-system-x86 \
	&& rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/bzImage /app/rootfs.img /app/
COPY --from=builder /app/output/ /app/output/
COPY --from=builder /app/*.ko /app/ 2>/dev/null || true

COPY scripts/docker-entrypoint.sh /app/
RUN chmod +x /app/docker-entrypoint.sh

WORKDIR /app
EXPOSE 2222

ENTRYPOINT ["/app/docker-entrypoint.sh"]
