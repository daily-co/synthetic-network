FROM node:16.16.0 as base

# Linux networking tools
RUN apt-get update && apt-get install -y \
    iproute2 ethtool iputils-ping iperf3 python3 lsof tcpdump net-tools

# Chrome dependency Instalation
RUN apt-get update && apt-get install -y \
    fonts-liberation \
    libasound2 \
    libatk-bridge2.0-0 \
    libatk1.0-0 \
    libatspi2.0-0 \
    libcups2 \
    libdbus-1-3 \
    libdrm2 \
    libgbm1 \
    libgtk-3-0 \
#    libgtk-4-1 \
    libnspr4 \
    libnss3 \
    libwayland-client0 \
    libxcomposite1 \
    libxdamage1 \
    libxfixes3 \
    libxkbcommon0 \
    libxrandr2 \
    xdg-utils \
    libu2f-udev \
    libvulkan1


# TigerVNC and ratposion
# build with `--build-arg VNC=true`
# (you can run for example:
#    /opt/lib/run-in-vnc.sh chromium --disable-gpu --no-sandbox
#  to have a VNC server with chromium listening on port 5901)
ADD run-in-vnc.sh /opt/lib/
ARG VNC
RUN if [ -n "$VNC" ] ; then apt-get install -y tigervnc-standalone-server ratpoison ; else echo "No VNC for you" ; fi

# Rust nightly
RUN apt-get install -y curl build-essential
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- --default-toolchain nightly -y
ENV PATH=${PATH}:/root/.cargo/bin

# Add and build rush (synthetic network backend / userspace proxy)
ADD rush /opt/src/rush
WORKDIR /opt/src/rush
RUN cargo build --release
RUN cp /opt/src/rush/target/release/rush /opt/lib/rush

# Add frontend (synthetic network web UI)
ADD frontend /opt/lib/frontend

# Entrypoint / test setup
ADD setup.sh /opt/lib/
WORKDIR /opt/lib
CMD ["/opt/lib/setup.sh"]
