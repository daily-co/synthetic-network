FROM syntheticnet:vnc

RUN wget -q https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb
RUN dpkg -i google-chrome-stable_current_amd64.deb || apt-get --fix-broken install -y

ENV USE_VNC=yes
ENV ENTRY="google-chrome-stable \
    --no-sandbox --disable-gpu --no-first-run \ 
    --disable-dev-shm-usage --disable-sync \
    --autoplay-policy=no-user-gesture-required  \
    --use-fake-ui-for-media-stream --use-fake-device-for-media-stream \
    --user-data-dir=/tmp/chrome --remote-debugging-port=9222 \
    http://localhost"

EXPOSE 5901/tcp
