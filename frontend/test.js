const Frontend = require('./frontend')
const SyntheticNetwork = require('./index')

const { spawn } = require('child_process')

// Run test
test()

async function test(port) {

    const specpath = "spec.json"
    const frontendPort = 8080
    const rushdummy = spawn("./rushdummy.sh", [specpath])
    rushdummy.stdout.on('data', (data) => console.log(`config: ${data}`))
    rushdummy.stderr.on('data', (data) => console.log(`error: ${data}`))

    const frontend = await Frontend.start(specpath, rushdummy.pid, frontendPort)

    const synthnet = new SyntheticNetwork({
        hostname: "localhost",
        port: frontendPort
    })
    await synthnet.commit() // Clear

    await synthnet.get()
    console.log("Current ingress rate (default flow)",
                synthnet.default_link.ingress.rate())

    await synthnet.commit()
    await sleep(1000)

    synthnet.default_link.ingress.rate(100000)
    synthnet.default_link.egress.rate(100000)
    await synthnet.commit()
    await sleep(1000)

    synthnet.addFlow('udp', {protocol: 'udp'})
    synthnet.flows.udp.link.ingress.rate(500000)
    synthnet.flows.udp.link.egress.rate(500000)
    synthnet.flows.udp.link.egress.loss(0.01)
    await synthnet.commit()
    await sleep(1000)

    synthnet.removeFlow('udp')
    await synthnet.commit()
    await sleep(1000)

    const egress = await synthnet.profiles.egress.get()
    for (var flow in egress.flows)
        console.log(flow, egress.flows[flow])

    frontend.close()
    rushdummy.kill()

}

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms))
}
