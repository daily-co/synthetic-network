# Frontend / NodeJS module

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

See also: [`./udp_rate_sine_demo.js`](udp_rate_sine_demo.js)

> A note about `/node_modules`... The choice to check-in dependencies is an intentional one. It is perhaps dogmatic but this repo follows a strict “vendor-everything” policy because dev ergonomics. Besides increasing the chances that things will Just Work™ for you, dear reader, it also releases us from being beholden to external package changes, updates, and breakages. 
