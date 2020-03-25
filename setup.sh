#!/usr/bin/env bash

set -e
#set -x

# Synthetic network setup
#
# Expects to run in a privileged docker container and the environment variable
# SYNTHETIC_NETWORK to be set to the subnet of the synthetic network. The
# container must be connected to that network.
#
# Example:
#  $ export SYNTHETIC_NETWORK=10.77.0.0/16
#  $ docker network create synthetic-network --subnet=$SYNTHETIC_NETWORK
#  $ docker create --privileged --env SYNTHETIC_NETWORK --name test <image>
#  $ docker network connect synthetic-network test
#
# Here, we take over the interface to the synthetic network and install a
# userspace proxy to shape the traffic. The application to be tested will use
# this conditioned interface (via a default route), and hence use the synthetic
# network. Services that we wish to remain uninterrupted (REST API to control
# the synthetic network, Chrome headless remote port, ...) will still listen on
# the default network.
#

# Fall back to using default network
if [ -z $SYNTHETIC_NETWORK ]; then
    export SYNTHETIC_NETWORK=$(ip route | grep -w eth0 | awk '{print $1}' | tail -n1)
fi
# Find the interface on SYNTHETIC_NETWORK
dev=$(ip route | grep -w $SYNTHETIC_NETWORK | awk '{print $3}')
if [ -z $dev ]; then
    echo "error: not connected to synthetic network"
    exit 1
fi
# Guess gateway on synthetic network
defaultRoute=$(echo $SYNTHETIC_NETWORK | sed 's/0\/.*/1/g')
# Get our address on the synthetic network
# (we will replicate this address on our virtual interface)
address=$(ip addr show dev $dev | grep -w inet | awk '{print $2}')
broadcast=$(ip addr show dev $dev | grep -w inet | awk '{print $4}')

# Remove address from $dev (having the same address on multiple interfaces
# would confuse Linux to no end)
ip address del $address dev $dev

# Remove the current default route
# (we will install a new one that routes through the synthetic network)
ip route del default || true

# Create a new veth pair veth0<->veth0s
# We will bridge $dev<->veth0:
#  - $dev is the container’s interface on the synthetic network
#  - userspace packet processing program forwards packets
#    between $dev and veth0
#  - the application to be tested will use veth0s for all its networking
#   (because we will install a default route that tells it to)
ip link add veth0 type veth peer name veth0s

# Bring veth0 up
ip link set veth0 up

# Configure our synthetic interface (veth0s)
# We assign the container’s IP address on the synthetic network to veth0s, and
# direct the container’s default route to go over veth0s as well so that the
# application to be tested defaults to the synthetic network
ip address add dev veth0s local $address broadcast $broadcast # Add IP
ip link set veth0s up # Bring veth0s up
ip route add default via $defaultRoute dev veth0s # Add Route


# Configure and start userspace proxy between interface to the
# synthetic network and veth
#

# Traffic profiles
ingress_profile=/var/run/synthetic_network-ingress.profile
egress_profile=/var/run/synthetic_network-egress.profile

# QoS config path
qos_spec=/opt/etc/synthetic_network.json
mkdir -p $(dirname $qos_spec)

# Set initial QoS spec
/opt/lib/rush/target/release/rush -h | grep "Example config" | awk '{print $5}' > $qos_spec

# Start rush (userspace network proxy) bridging $dev<->veth0
/opt/lib/rush/target/release/rush \
    $dev veth0 $qos_spec $ingress_profile $egress_profile &
rushpid=$!
# Trigger reload of $qos_spec with
#   kill -s SIGHUP $rushpid

# Start frontend listening on port 80
(cd /opt/lib/frontend/;
 node index.js $qos_spec $rushpid $ingress_profile $egress_profile 80) &


# Run tests!
#
# Defaults to executing /opt/bin/entry
# (put the executable/script that launches your test there)
# (( add USE_VNC=true to run VNC server ))
#
# However, you can set the ENTRY environment variable to override this.
# I.e., to test interactively, you could do:
#   $ docker create --privileged --env SYNTHETIC_NETWORK --name test \
#                   --env ENTRY=bash -t -i <image>
#   ...
#   $ docker start -a -i test
#

[ -z "$ENTRY" ] && ENTRY=/opt/bin/entry
if [ -n "$USE_VNC" ];then
	USE_VNC=/opt/lib/run-in-vnc.sh
else
	USE_VNC=""
fi

$USE_VNC $ENTRY


# Some interesting commands for testing:
#
# tcpdump -v -e -n -i veth0s
# python3 -m http.server 8080
# curl http://inters.co
# iperf3 -s/-c
# netstat -s
# netstat -l
