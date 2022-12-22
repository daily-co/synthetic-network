# Synthetic Network

Docker containers on a synthetic network. Run applications in a context that
lets you manipulate their network conditions.

## Dependencies

- [Docker](https://docs.docker.com/get-docker/)
- [make](https://www.gnu.org/software/make/)
- Optional: a VNC client ie. [TigerVNC](https://tigervnc.org/)

### Overview

```
$ make
SYNTHETIC_NETWORK ?= 10.77.0.0/16
CONTAINER_NAME_INTERACTIVE ?= syntheticnet-interactive
CONTAINER_NAME_CHROME ?= syntheticnet-chrome
TESTHOST ?= <hostname>:<address> (add /etc/hosts entry to container)
help: # Print this help message
image: # Build Docker image: syntheticnet
image-vnc: # Build Docker image: syntheticnet:vnc
image-chrome: image-vnc # Build Docker image: syntheticnet:chrome
rush: # Build rush
minimal: rush # Build Docker image: syntheticnet:minimal
run-interactive: image synthetic-network # Debug syntheticnet container. Prereq: create-synthetic-network
run-chrome: image-chrome synthetic-network # Run syntheticnet:chrome. Prereq: create-synthetic-network
synthetic-network: # Specify SYNTHETIC_NETWORK (this rule is documentation)
create-synthetic-network: synthetic-network # Create Docker network: synthetic-network
```

### Run Chrome using Synthetic Network in VNC
1. ensure Docker for Mac/Windows/Linux is running

2.
```
$ make create-synthetic-network # You only need to do this once
$ make run-chrome
...
🎛 Synthetic network GUI will listen on http://localhost:3000

📺 Point your VNC client at localhost:5901
...
```
3. open TigerVNC and navigate to 127.0.0.1::5901

#### Resolving test domains within the container

```
$ TESTHOST=my-test-domain.dev:192.168.0.1 make run-chrome
```

### Build Container Image

Build `syntheticnet` image

```$ make image```

with VNC:

```$ make image-vnc```

It is also possible to build a minimal image with:

```$ make minimal```

The `minimal` image doesn't have a frontend to update the user network proxy
(`rush`) settings (see example below). Instead, one can create a configuration
file and place it at `/opt/etc/synthetic_network.json` which can be done by
creating a new container image based on the `minimal` image and copying a file
to that particular location. If no configuration file is provided the example
settings will be used (see `rush -h` below).

### Rush settings example

[`rush/`](rush) is the packet processing framework we use to handle bandwidth,
packet loss, jitter, etc. This is an example configuration:

```
{
  "default_link": {
    "ingress": {
      "rate": 10000000,
      "loss": 0,
      "latency": 0,
      "jitter": 0,
      "jitter_strength": 0,
      "reorder_packets": false
    },
    "egress": {
      "rate": 10000000,
      "loss": 0.02,
      "latency": 0,
      "jitter": 0,
      "jitter_strength": 0,
      "reorder_packets": false
    }
  },
  "flows": []
}
```

Where we define a 10Mbps ingress and egress network and a 2% packet loss on
egress. We can also specify settings per each protocol, an example can be
obtained running `rush` help:

```
make rush
cd rush
cargo build --release
./target/release/rush -h
```

## Scripting the Synthetic Network

```js
const SyntheticNetwork = require('synthetic-network/frontend')

const synthnet = new SyntheticNetwork({hostname: "localhost", port: 3000})

await synthnet.get() // Get current configuration

// Double ingress rate
var current_ingress_rate = synthnet.default_link.ingress.rate()
synthnet.default_link.ingress.rate(current_ingress_rate*2)

await synthnet.commit() // Apply new configuration

// Add a flow
synthnet.addFlow('udp', {protocol: 'udp'})
synthnet.flows.udp.link.ingress.rate(500000)
synthnet.flows.udp.link.egress.rate(500000)
synthnet.flows.udp.link.egress.loss(0.01)
await synthnet.commit()

// Print ingress traffic statistics
const ingress_profile = await synthnet.profiles.ingress.get()
for (var flow in ingress_profile.flows)
  console.log(flow, ingress_profile.flows[flow].packets)

// ...
```

See also: [`frontend/udp_rate_sine_demo.js`](frontend/udp_rate_sine_demo.js)

## Further reading

Check out the reports under [`doc/`](doc) for details.

The packet processing framework we use to do network conditioning can be found
under [`rush/`](rush). Its README points to a screen cast series covering its design
and implementation.
