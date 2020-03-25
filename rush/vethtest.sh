#!/usr/bin/env bash

set -e
set -x

BITRATE=1000M
[ -z $1 ] || BITRATE=$1

tid=$$

veth0=veth0_${tid}
veth1=veth1_${tid}
veth0u=${veth0}_u
veth1u=${veth1}_u

ns0=ns0_${tid}
ns1=ns1_${tid}

(sleep 60
 echo "Test timeout!"
 kill -SIGTERM $$
 # Need to really kill it for Travis!?
 # sleep 1
 # kill -SIGKILL $$
) &
timeout=$!

iperf=$(which iperf || which iperf3)
iperfs="" # To be assigned
rushproc="" # To be assigned

function cleanup {
    kill $timeout || true
    [ -z $iperfs ] || kill $iperfs || true
    [ -z $rushproc ] || kill $rushproc || true
    ip link delete $veth0u || true
    ip link delete $veth1u || true
    ip netns delete $ns0 || true
    ip netns delete $ns1 || true
}
trap cleanup EXIT HUP INT QUIT TERM

# Test setup

ip link add $veth0 type veth peer name $veth0u
ip link add $veth1 type veth peer name $veth1u

ip link set $veth0u up
ip link set $veth1u up

ip netns add $ns0
ip netns add $ns1
ip netns exec $ns0 ip link set lo up
ip netns exec $ns1 ip link set lo up

ip link set $veth0 netns $ns0
ip link set $veth1 netns $ns1

#ip netns exec $ns0 ethtool --offload $veth0  rx off tx off
ip netns exec $ns0 ip address add dev $veth0 local 10.10.0.1/24
# ip netns exec $ns0 ip link set $veth0 mtu 1440
ip netns exec $ns0 ip link set $veth0 up
# ip netns exec $ns0 ip route add 10.10.0.0/24 via 10.10.0.2 src 10.10.0.1 dev $veth0
ip netns exec $ns0 ip route add default via 10.10.0.2 dev $veth0

#ip netns exec $ns1 ethtool --offload $veth1  rx off tx off
ip netns exec $ns1 ip address add dev $veth1 local 10.10.0.2/24
# ip netns exec $ns1 ip link set $veth1 mtu 1440
ip netns exec $ns1 ip link set $veth1 up
# ip netns exec $ns1 ip route add 10.10.0.0/24 via 10.10.0.1 src 10.10.0.2 dev $veth1
ip netns exec $ns1 ip route add default via 10.10.0.1 dev $veth1

# Start rush (userspace network proxy)
target/release/rush $veth0u $veth1u spec.conf \
                    top_ingress.profile top_egress.profile &
rushproc=$!

sleep 1
tee spec.conf <<EOF
{"default_link":
 {"ingress":{
   "rate":10000000,"loss":0.01,"latency":50,"jitter":10,"jitter_strength":0.3,"reorder_packets":false},
  "egress":{
   "rate":1000000,"loss":0.05,"latency":50,"jitter":20,"jitter_strength":0.3,"reorder_packets":false}},
 "flows":[{"label":"udp","flow":{"ip":0,"protocol":17,"port_min":0,"port_max":65535},
           "link":{"ingress":{"rate":1000000000,"loss":0.0,"latency":0,"jitter":0,"jitter_strength":0.0,"reorder_packets":false},
                   "egress":{"rate":1000000000,"loss":0.0,"latency":0,"jitter":0,"jitter_strength":0.0,"reorder_packets":false}}}]}
EOF
kill -s SIGHUP $rushproc

# Wait until link is established
until ip netns exec $ns0 ping -c 1 10.10.0.2; do sleep 1; done

# Start iperf server
ip netns exec $ns0 $iperf -s &
iperfs=$!

# Test iperf
sleep 2
ip netns exec $ns1 $iperf -c 10.10.0.1 -u -b $BITRATE

ip netns exec $ns0 ping -c 10 10.10.0.2

tee spec.conf <<EOF
{"default_link":
 {"ingress":{
   "rate":10000000,"loss":0,"latency":15,"jitter":3,"jitter_strength":0.3,"reorder_packets":false},
  "egress":{
   "rate":1000000,"loss":0,"latency":15,"jitter":5,"jitter_strength":0.3,"reorder_packets":false}},
 "flows":[{"label":"udp","flow":{"ip":0,"protocol":17,"port_min":0,"port_max":65535},
           "link":{"ingress":{"rate":1000000000,"loss":0.0,"latency":0,"jitter":0,"jitter_strength":0.0,"reorder_packets":false},
                   "egress":{"rate":1000000000,"loss":0.0,"latency":0,"jitter":0,"jitter_strength":0.0,"reorder_packets":false}}}]}
EOF
kill -s SIGHUP $rushproc

ip netns exec $ns0 ping -c 10 10.10.0.2

kill -s SIGHUP $rushproc