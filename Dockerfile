ARG BUILD_ROOT=/root
ARG BUILD_OUTPUT_DIR=${BUILD_ROOT}/solana-output

FROM ghcr.io/ferumlabs/blackwing-svm/validator-builder:v0.1.0 as cli

ARG BUILD_ROOT

# Install cli
COPY . /data/
WORKDIR /data
RUN cargo install --path /data/cli --root ${BUILD_ROOT}/.cargo --features localnet

FROM ghcr.io/ferumlabs/blackwing-svm/final-base:v1.0.0 as final

ARG BUILD_ROOT
ARG BUILD_OUTPUT_DIR

COPY --from=cli ${BUILD_OUTPUT_DIR} /usr/
COPY --from=cli ${BUILD_ROOT}/.cargo /usr/.cargo

ENV PATH=/usr/.cargo/bin:$PATH

COPY . /data/
WORKDIR /data

# RPC JSON
EXPOSE 8899/tcp
# RPC pubsub
EXPOSE 8900/tcp
# entrypoint
EXPOSE 8001/tcp
# (future) bank service
EXPOSE 8901/tcp
# bank service
EXPOSE 8902/tcp
# faucet
EXPOSE 9900/tcp
# tvu
EXPOSE 8000/udp
# gossip
EXPOSE 8001/udp
# tvu_forwards
EXPOSE 8002/udp
# tpu
EXPOSE 8003/udp
# tpu_forwards
EXPOSE 8004/udp
# retransmit
EXPOSE 8005/udp
# repair
EXPOSE 8006/udp
# serve_repair
EXPOSE 8007/udp
# broadcast
EXPOSE 8008/udp
# tpu_vote
EXPOSE 8009/udp

ENV SHELL=/bin/bash
ENV LANG=C.UTF-8 LANGUAGE=C.UTF-8 LC_ALL=C.UTF-8

WORKDIR /data
ENTRYPOINT [ "/bin/bash", "-c" ]
CMD [ "solana-test-validator \
--bpf-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s /data/deploy_local_fixtures/metaplex_token_metadata_program.so \
--bpf-program CVHAgHmDhZRjscris8N18pgwf8YS5WGjNUaNvhS7TC9e /data/deploy_local_fixtures/raydium_cp_swap.so \
--bpf-program 6TvznH3B2e3p2mbhufNBpgSrLx6UkgvxtVQvopEZ2kuH /data/target/deploy/limitless.so \
--bpf-program 5vcNSdph649wMexPUZsQBF1b9RamYw83RHeKtCyr93mX /data/target/deploy/creator_v2.so" ]