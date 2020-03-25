#!/usr/bin/env bash

set -e

# RUN THIS ONLY IN A CONTAINER
# BE AWARE THAT YOU SHOULD NOT PUBLISH THE CONTAINER’S VNC PORT (5901)
# TO THE DANGEROUS PLACE THAT IS THE INTERNET

mkdir -p /opt/etc

cat > /opt/etc/ratpoison.cfg <<EOF
EOF
cat > /opt/etc/xstartup.sh <<EOF
#!/usr/bin/env bash
vncconfig -nowin & # https://groups.google.com/g/tigervnc-users/c/TikOA7hCZEw?pli=1
touch /opt/etc/xstarted
exec ratpoison -f /opt/etc/ratpoison.cfg
EOF
chmod +x /opt/etc/xstartup.sh
vncserver -name "$1" -fg -autokill \
          -localhost no --I-KNOW-THIS-IS-INSECURE -SecurityTypes None \
          -xstartup /opt/etc/xstartup.sh &

until test -f /opt/etc/xstarted; do sleep .1; done
DISPLAY=:1 $@
