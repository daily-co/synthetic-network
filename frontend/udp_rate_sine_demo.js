const SyntheticNetwork = require('./index')

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms))
}

async function demo(host, port, min_mbps, max_mbps) {
  const synthnet = new SyntheticNetwork({hostname: host, port: port})
  const Mbps = 1000000

  // Initial config
  synthnet.default_link.ingress.rate(100*Mbps)
  synthnet.default_link.egress.rate(100*Mbps)
  synthnet.addFlow('udp', {protocol: 'udp'})
  synthnet.flows.udp.link.ingress.rate(max_mbps*Mbps)
  synthnet.flows.udp.link.egress.rate(max_mbps*Mbps)
  await synthnet.commit()

  await sleep(1000)

  // Modulate UDP rate following a ∿ sine pattern (period=10s)
  //
  //     y = sin x
  //
  //  1 -|         ,-'''-.
  //     |      ,-'       `-.
  //     |    ,'             `.
  //     |  ,'                 `.
  //     | /                     \
  //     |/                       \
  // ----+-------------------------\--------------------------
  //     |                          \                       /
  //     |           π/2          π  \         3π/2        /  2π
  //     |                            `.                 ,'
  //     |                              `.             ,'
  //     |                                `-.       ,-'
  // -1 -|                                   `-,,,-'
  //
  // See https://en.wikipedia.org/wiki/Sine
  //
  const base_rate = min_mbps
  const rate_delta = max_mbps-min_mbps
  var tick = 0
  var period = 10
  const tickRate = (tick) => {
    const scale = (Math.sin(Math.PI*(tick/period))+1)/2
    return base_rate + rate_delta*scale
  }
  while (true) {
    const rate = tickRate(tick++)
    synthnet.flows.udp.link.ingress.rate(rate*Mbps)
    synthnet.flows.udp.link.egress.rate(rate*Mbps)
    await synthnet.commit()
    await sleep(1000)
  }

}

demo("localhost", 3000, 0.5, 10)
