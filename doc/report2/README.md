# Synthetic network for Docker containers

We’ve added a few more apps
([Latency](https://github.com/daily-co/synthetic-network/blob/main/rush/src/qos.rs#L47-L90),
[Jitter](https://github.com/daily-co/synthetic-network/blob/main/rush/src/qos.rs#L92-L151),
and [RateLimiter](https://github.com/daily-co/synthetic-network/blob/main/rush/src/qos.rs#L221-L319)),
moved our playground `main()` function into a basic
[program](https://github.com/daily-co/synthetic-network/blob/main/rush/src/synthetic_network.rs#L31-L66),
and added a fledgling
[frontend](https://github.com/daily-co/synthetic-network/tree/main/frontend)
to control our synthetic network.

We also sketched a [Dockerfile](https://github.com/daily-co/synthetic-network/blob/main/Dockerfile)
that builds an image providing a synthetic network to applications running
within.

~~None of this worked on the first try, and we’re integrating all those
components and debugging them on the
`daily-frontend-integrate`
branch. Checkout this branch to follow along the examples below.~~

## The synthetic network program

By now, `cargo build --release` builds our synthetic network program. If you
run it without arguments it will print an error about how you called it with an
invalid number of arguments, but also print useful usage instructions.

```
$ target/release/rush
Invalid number of arguments.
Usage: target/release/rush <outer_ifname> <inner_ifname> <specpath>
Example config for <specpath>:
(Note: I have formatted this output a bit for readability)
{
  "ingress": {"rate": 10000000,
              "loss":0.0,
              "latency":0,
              "jitter":0,
              "jitter_strength":0.0},
  "egress": {"rate":1000000,
             "loss":0.0,
             "latency":0,
             "jitter":0,
             "jitter_strength":0.0}
}
```

So this program bridges two Linux network interfaces (*outer_ifname*,
*inner_ifname*) and simulates a possible degraded link as specified in the JSON
configuration at *specpath*.

Ingress and egress (outer→inner and inner→outer) flows can be conditioned
separately, but their respective configuration options are identical, and as
follows:


- `rate` - maximum bitrate/throughput (in the example 10 Mbit/s ingress, and
  1 Mbit/s egress)
  
- `loss` - packet loss (0.0 means 0%, 0.1 means 10%, etc.)

- `latency` - constant added latency in milliseconds (a value of 100 would mean
  100 ms)
  
- `jitter` - random latency in milliseconds

- `jitter_strength` - ratio of packets affected by jitter (0.1 would mean 10%
   of all packets are delayed for randomly distributed duration between 0 and
  `jitter` milliseconds)

### Forcing a reload of the JSON configuration

If you send the `SIGHUP` signal to the synthetic network program it will reload
and apply the new configuration at *specpath*.

This should incur only minimal interruption of service. Hopefully short enough
that we won’t notice, but the synthetic network program does stop processing
packets while it reads and applies the new JSON configuration, so the
interruption will definitely be measurable with precise tools.

## The frontend

There is a frontend for the synthetic network program under the `frontend/`
directory. It’s a super simplistic NodeJS/Express application which is
currently not self-documenting, sorry! Usage goes as follows:

```
$ cd frontend/
$ node start.js <specpath> <pid> [<port>]
```

Where:

 - *specpath* - Path to the JSON configuration read by the synthetic network
   program

 - *pid* - the PID of the synthetic network program (to send `SIGHUP` to)
 
 - *port* - (Optional) a port to listen on for HTTP requests (defaults to 8080)
 
![Frontend](frontend.jpg)

The frontend has a simple HTML UI to manipulate the JSON configuration for the
synthetic network program. Every change in the UI sends a SIGHUP signal (i.e.,
triggers a state-change) to the packet forwarding engine after writing the new
configuration.

### “REST” API

You can achieve the same as above without using the HTML UI by sending a `POST`
request to `/` with `Content-Type: application/json` and a JSON payload that
matches the configuration of the synthetic network program.

## Synthetic network in a Docker container

We include a *Dockerfile* for creating containers connected to the synthetic
network in which you can run applications under simulated network conditions.
From within the repository root, you can build the image like so:

```
$ docker build -t syntheticnet .
...
Successfully built d60acd199325
Successfully tagged syntheticnet:latest
```

To run containers using the image we’ve just created we first need to create a
dedicated network that will serve as our “synthetic network”. We will connect
our containers to this network in addition to the docker default network, so we
can use the default network as a control channel (e.g., to access the frontend,
or drive our tests).

```
$ export SYNTHETIC_NETWORK=10.77.0.0/16 # a subnet of your choice
$ docker network create synthetic-network --subnet=$SYNTHETIC_NETWORK
```

> If the subnet is already assigned on your host (by docker or otherwise), I
> would hope that `docker network create` would signal an error.

To create a new container to test the synthetic network interactively we could
do the following:

```
$ docker create --privileged \
                --env SYNTHETIC_NETWORK \
                --publish 3000:80 \
                --name test \
                --tty --interactive --env ENTRY=bash \
                syntheticnet
$ docker network connect synthetic-network test
```

Let’s explain those command line flags:

- `--privileged`: we need the container to privileged because we need to create
  new virtual interfaces, change the settings on existing ones, and be able to
  capture packets from interfaces via `PACKET_RAW` (Might be that the actual
  capabilities we need are just `CAP_NET_RAW` and `CAP_NET_ADMIN`?)

- `--env SYNTHETIC_NETWORK`: here we pass the subnet we want to be the
  synthetic network to the container’s environment, see the `export` when we
  created the synthetic network (`setup.sh` needs this to figure out which
  interface to “take over”)

- `--publish 3000:80`: expose container port 80 to host port 3000 so that the
  frontend will be reachable on `http://localhost:3000`
  
- `--name test`: we’ll name this container “test”

- `--tty --interactive --env ENTRY=bash`: these flags make it so that we can
  play around with the container interactively (by setting `ENTRY=bash` we tell
  the container to run a BASH shell after setting up the synthetic network
  link and starting the frontend, more on the `ENTRY` environment variable
  later)

- `syntheticnet`: finally, this is the name of the image we create the
  container from

After we connected the container to the synthetic network with
`docker network connect synthetic-network test` we can start it like so:

```
$ docker start --attach --interactive test
...
root@b68cad1af778:/opt/lib#
```

…which will drop us into a bash shell within the container. You should now be
able to visit [localhost:3000](http://localhost:3000) on the host to reach the
Web frontend controlling the container’s link.

You can test the network by running…

```
$ ping inters.co
```

…for example. You should also be able to e.g. change the latency in the
frontend and see it affect the ping round-trip time. (Make sure you set a
positive rate limit, otherwise the pings won’t get through.)

### Extending the syntheticnet image

Currently, our idea to consume the image is by deriving your test image from
the *syntheticnet* image, by starting your *Dockerfile* with

```
FROM syntheticnet
...
```

If the `ENTRY` environment variable is not set, the *syntheticnet* image will
try to execute `/opt/bin/entry` after setting up the synthetic network and
starting the frontend. You could `COPY` your test runner to that path in your
derived *Dockerfile*.

### Advanced tests

You can get the container’s IP address on the synthetic network via:

```
root@b68cad1af778:/opt/lib# ip addr show veth0s
2: veth0s@veth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000
    link/ether 7a:7f:7c:c1:8b:77 brd ff:ff:ff:ff:ff:ff
    inet 10.77.0.2/16 brd 10.77.255.255 scope global veth0s
       valid_lft forever preferred_lft forever
```

You could create a second container with a different name, and a different
publish port for the web interface (heads up: make sure `$SYNTHETIC_NETWORK`
set in your environment):

```
$ docker create --privileged \
                --env SYNTHETIC_NETWORK \
                --publish 3001:80 \
                --name test2 \
                --tty --interactive --env ENTRY=bash \
                syntheticnet
$ docker network connect synthetic-network test2
$ docker start --attach --interactive test2
```

With both containers running, you could run an *iperf* server one one of them…

```
# iperf3 -s
```

…and an *iperf* client on the other…

```
# iperf3 -c 10.77.0.2
```

…and observe its behavior while tweaking the synthetic network in the frontend
of each container respectively. Some other useful *iperf* client flags include:

 - `-u -b 100M` to transmit 100 Mbit/s of UDP traffic (*iperf* defaults to TCP)
 - `-t 300` to run the *iperf* client for 5 minutes (300 seconds) instead of
   the usual 10 seconds

Some more interesting commands for testing for your inspiration:

```
$ tcpdump -v -e -n -i veth0s    # log packets on synthetic network
$ python3 -m http.server 8080   # start a HTTP server
$ curl http://inters.co         # do a HTTP request
$ netstat -s                    # print network statistics (debug TCP problems)
$ netstat -l                    # show active sockets
```
