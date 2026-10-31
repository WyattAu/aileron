# Containerfile — aileron-agent (headless MCP browser daemon)
#
# Builds the agent profile and ships a minimal runtime: the daemon binary,
# WebKitGTK + GTK libs it needs, and Xvfb. The MCP relay listens on
# 127.0.0.1:9543 inside the container; publish/forward as needed.
#
# Build:  podman build -t aileron-agent -f Containerfile .
# Run:    podman run -p 127.0.0.1:9543:9543 aileron-agent
#
# NOTE: untested in this environment (no container runtime installed) —
# first build + smoke run is the gate (see docs/sprints/ERA2-2028.md).

FROM archlinux:base-devel AS build
RUN pacman -Syu --noconfirm rust cargo clang pkg-config \
    gtk3 webkit2gtk-4.1 xorg-server-xvfb
WORKDIR /src
COPY . .
# The agent profile needs no chrome/terminal build artifacts.
RUN cargo build --bin aileron --bin aileron-mcp

FROM archlinux:base
RUN pacman -Syu --noconfirm \
    gtk3 webkit2gtk-4.1 xorg-server-xvfb \
    sqlite enchant nss && pacman -Scc --noconfirm
COPY --from=build /src/target/debug/aileron /usr/local/bin/aileron
COPY --from=build /src/target/debug/aileron-mcp /usr/local/bin/aileron-mcp
COPY --from=build /src/scripts/aileron-mcp /usr/local/bin/aileron-mcp-wrapper
ENV LD_LIBRARY_PATH=/usr/lib
ENV RUST_LOG=info
# Container runs its own Xvfb; the MCP port is the only interface.
EXPOSE 9543
CMD ["sh", "-c", "Xvfb :92 -screen 0 1280x800x24 & sleep 1; DISPLAY=:92 aileron --agent"]
