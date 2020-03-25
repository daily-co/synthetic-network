'use strict'

/* NodeJS/Browser polyglot */
var __nodejs = typeof module == "object" && this === module.exports

if (__nodejs) module.exports = SyntheticNetwork

var flowlib = (__nodejs && require('./flow')) || flowlib
const http = __nodejs && require('http')

// See ../rush/src/synthetic_network.rs

function SyntheticNetwork(endpoint) {
    this._endpoint = endpoint
    this.default_link = new SyntheticLink()
    this.default_flow = this.default_link
    this.default = this.default_link
    this._flows = []
    this.flows = {}

    this.profiles = {
        ingress: new Profile(endpoint, 'ingress'),
        egress: new Profile(endpoint, 'egress')
    }

    this.addFlow = (label, flow) => {
        if (this.flows[label]) throw `Flow '${label}' already exists`
        this.flows[label] = new SyntheticFlow(label, flow)
        this._flows.push(this.flows[label])
    }
    this.flowIndex = (label) => {
        return this._flows.findIndex(flow => flow == this.flows[label]) || -1
    }
    this.removeFlow = (label) => {
        this._flows.splice(this.flowIndex(label), 1)
        this.flows[label] = undefined
    }
    this.matchFlow = (flow) => {
        for (var r of this._flows)
            if ((r.flow.ip == 0 || r.flow.ip == flow.ip) &&
                (r.flow.protocol == 0 || r.flow.protocol == flow.protocol) &&
                r.flow.port_min <= flow.port_min &&
                r.flow.port_max >= flow.port_max)
                return r.label
    }

    this.get = () => {
        return new Promise(resolve => {
            this._endpoint.path = '/qos'
            this._endpoint.method = 'GET'
            this._endpoint.headers = {'Content-Type': 'application/json'}
            if (__nodejs) {
                const req = http.get(this._endpoint, (res) => {
                    var data = ''
                    res.on('data', (chunk) => data += chunk)
                    res.on('end', () => resolve(this.update(JSON.parse(data))))
                    req.on('error', e => {throw `Failed to GET: ${e.message}`})
                })
            } else {
                const xhttp = new XMLHttpRequest()
                xhttp.open(this._endpoint.method, this._endpoint.path, true)
                for (var header in this._endpoint.headers)
                    xhttp.setRequestHeader(header, this._endpoint.headers[header])
                xhttp.onreadystatechange = () => {
                    if (xhttp.readyState == XMLHttpRequest.DONE && xhttp.status == 200)
                        resolve(this.update(JSON.parse(xhttp.responseText)))
                }
                xhttp.send()
            }
        })
    }

    this.post = () => {
        return new Promise(resolve => {
            const data = JSON.stringify(this.config())
            this._endpoint.path = '/qos'
            this._endpoint.method = 'POST'
            this._endpoint.headers = {
                'Content-Type': 'application/json',
                'Content-Length': data.length
            }
            if (__nodejs) {
                const req = http.request(this._endpoint, resolve)
                req.on('error', e => {throw `Failed to POST: ${e.message}`})
                req.write(data)
                req.end()
            } else {
                const xhttp = new XMLHttpRequest();
                xhttp.open(this._endpoint.method, this._endpoint.path, true)
                xhttp.setRequestHeader('Content-Type', 'application/json')
                xhttp.send(data)
                xhttp.onreadystatechange = () => {
                    if (xhttp.readyState == XMLHttpRequest.DONE && xhttp.status == 200)
                        resolve()
                }
            }
        })
    }
    this.commit = this.post

    this.config = () => {
        return {default_link: this.default_link.config(),
                flows: this._flows.map(flow => flow.config())}
    }

    this.update = (obj) => {
        if (obj.default_link)
            this.default_link.update(obj.default_link)
        if (obj.flows) {
            this._flows = []
            this.flows = {}
            obj.flows.forEach((flow) => {
                this.addFlow(flow.label, {
                    ip: flowlib.ntopIPv4(flow.flow.ip),
                    protocol: flow.flow.protocol,
                    port_min: flow.flow.port_min,
                    port_max: flow.flow.port_max
                })
                this.flows[flow.label].link.update(flow.link)
            })
        }
    }
}

function SyntheticLink() {
    this.ingress = new QoS()
    this.egress = new QoS()
    this.config = () => {
        return {ingress: this.ingress.config(),
                egress: this.egress.config()}
    }
    this.update = (obj) => {
        if (obj.ingress) this.ingress.update(obj.ingress)
        if (obj.egress) this.egress.update(obj.egress)
    }
}

function QoS() {
    this._state = {
        rate: 100e6, // Default to generous 100 Mbps
        loss: 0.0,
        latency: 0,
        jitter: 0,
        jitter_strength: 0.0,
        reorder_packets: false
    }
    this.rate = (rate) => {
        if (rate != undefined) {
            if (rate < 0) throw "Rate must be positive"
            this._state.rate = Math.round(rate)
        }
        return this._state.rate
    }
    this.loss = (loss) => {
        if (loss != undefined) {
            if (loss < 0) throw "Loss must not be negative"
            if (loss > 1) throw "Loss must be a ratio within 0..1"
            this._state.loss = loss
        }
        return this._state.loss
    }
    this.latency = (latency) => {
        if (latency != undefined) {
            if (latency < 0) throw "Latency must not be negative"
            this._state.latency = Math.round(latency)
        }
        return this._state.latency
    }
    this.jitter = (jitter) => {
        if (jitter != undefined) {
            if (jitter < 0) throw "Jitter must not be negative"
            this._state.jitter = Math.round(jitter)
        }
        return this._state.jitter
    }
    this.jitter_strength = (strength) => {
        if (strength != undefined) {
            if (strength < 0) throw "Jitter strength must not be negative"
            if (strength > 1) throw "Jitter strength must be a ratio within 0..1"
            this._state.jitter_strength = strength
        }
        return this._state.jitter_strength
    }
    this.reorder_packets = (reorder) => {
        if (reorder != undefined) {
            if (typeof(reorder) != 'boolean') throw "Reorder packets must be a boolean"
            this._state.reorder_packets = reorder
        }
        return this._state.reorder_packets
    }
    this.config = () => {
        return this._state
    }
    this.update = (obj) => {
        for (const key in obj) {
            if (!this[key]) throw `No such field: ${key}`
            this[key](obj[key])
        }
    }
}

function SyntheticFlow(label, flow) {
    this.label = flowlib.parseLabel(label)
    this.flow = {
        ip: flowlib.ptonIPv4(flow.ip || "0.0.0.0"),
        protocol: flowlib.parseProtocol(flow.protocol || 0),
        port_min: flowlib.parsePort(flow.port_min || 0),
        port_max: flowlib.parsePort(flow.port_max || 65535)
    }
    this.link = new SyntheticLink()
    this.config = () => {
        return {label: this.label,
                flow: this.flow,
                link: this.link.config()}
    }
}


// See ../../rush/src/flow.rs: flow::Top

function Profile(endpoint, profile) {
    this._endpoint = {
        hostname: endpoint.hostname,
        port: endpoint.port,
        path: `/top/${profile}.profile`,
        method: 'GET',
        headers: {'Content-Type': 'application/octet-stream'},
        encoding: null
    }

    this.get = () => {
        return this._get().then((profile) => { return this._parse(profile) })
    }

    this._get = () => {
        return new Promise(resolve => {
            if (__nodejs) {
                const req = http.get(this._endpoint, (res) => {
                    var chunks = []
                    res.on('data', (chunk) => chunks.push(chunk))
                    res.on('end', () => resolve(Buffer.concat(chunks)))
                })
                req.on('error', e => {throw `Failed to GET: ${e.message}`})
            } else {
                const xhttp = new XMLHttpRequest()
                xhttp.open(this._endpoint.method, this._endpoint.path, true)
                for (var header in this._endpoint.headers)
                    xhttp.setRequestHeader(header, this._endpoint.headers[header])
                xhttp.responseType = 'arraybuffer'
                xhttp.onreadystatechange = () => {
                    if (xhttp.readyState == XMLHttpRequest.DONE
                        && xhttp.status == 200)
                    { resolve(xhttp.response) }
                }
                xhttp.send()
            }
        })
    }
    this._parse = (profile) => {
        const snapshot = {
            timestamp: Date.now(),
            flows: {}
        }
        const dv = !__nodejs && new DataView(profile)
        for (var i=0; i<2048; i++) {
            var packets, bits, id
            if (__nodejs) {
                packets = profile.readBigInt64LE(i*24+0)
                bits    = profile.readBigInt64LE(i*24+8)
                id      = profile.readBigInt64LE(i*24+16)
            } else {
                packets = dv.getBigUint64(i*24+0, true)
                bits    = dv.getBigUint64(i*24+8, true)
                id      = dv.getBigUint64(i*24+16, true)
            }
            if (packets > 0) {
                const [ip, protocol, port] = [
                    Number((id >> 0n) & 0xffffffffn),
                    Number((id >> 32n) & 0xffn),
                    Number((id >> 48n) & 0xffffn)
                ]
                const flow = { ip: ip,
                               protocol: protocol,
                               port_min: port,
                               port_max: port > 0 ? port : 65535 }
                snapshot.flows[flowlib.flowString(flow)] = {
                    packets: Number(packets),
                    bits: Number(bits),
                    flow: flow
                }
            }
        }
        snapshot.diff = (prev_snapshot) => {
            const elapsed = (snapshot.timestamp - prev_snapshot.timestamp) / 1000
            const d = []
            Object.keys(snapshot.flows).forEach(id => {
                const flow = snapshot.flows[id]
                const prev = prev_snapshot.flows[id]
                if (prev && elapsed > 0) {
                    const packets = flow.packets - prev.packets
                    const bits = flow.bits - prev.bits
                    if (packets > 0 && bits > 0) d.push({
                        pps: Math.round(packets / elapsed), // packets per second
                        bps: Math.round(bits / elapsed),    // bits per second
                        bpp: flowlib.avgBytesPerPacket(packets, bits), // bytes per packet
                        flow: flow.flow
                    })
                }
            })
            d.sort((x,y) => y.bps - x.bps)
            return d
        }
        return snapshot
    }
}

