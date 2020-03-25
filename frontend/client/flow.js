'use strict'

/* NodeJS/Browser polyglot */
var __nodejs = typeof module == "object" && this === module.exports

var flowlib = {
    parseFlow: parseFlow,
    flowString: flowString,
    ptonIPv4: ptonIPv4,
    ntopIPv4: ntopIPv4,
    parseProtocol: parseProtocol,
    parsePort: parsePort,
    parseLabel: parseLabel,
    avgBytesPerPacket: avgBytesPerPacket
}

if (__nodejs) module.exports = flowlib

// Convert from flow expression string to flow spec understood by backend and
// vice-versa

const Protocols = {
    1: 'icmp', icmp: 1,
    6: 'tcp', tcp: 6,
    17: 'udp', udp: 17
}

function parseFlow(str) {
    const fexp = /^([^:]+)(?::([^:]+)(?::(.+))?)?$/
    const match = fexp.exec(str)
    if (!match) throw "Invalid flow expression"
    var [_,ip,protocol,ports] = match

    if (ip == '*') {
        ip = 0
    } else {
        ip = ptonIPv4(ip)
    }

    if (protocol == undefined || protocol == '*') {
        protocol = 0
    } else {
        protocol = parseProtocol(parseInt(protocol, 10) || protocol)
    }

    var port_min, port_max
    if (ports == undefined || ports == '*') {
        [port_min,port_max] = [0,65535]
    } else {
        [port_min,port_max] = parsePortRange(ports)
    }
    if (port_max > 65535) throw "Invalid port"

    return {
        ip: ip,
        protocol: protocol,
        port_min: port_min,
        port_max: port_max
    }
}

function flowString(flow) {
    var ip = '*'
    if (flow.ip > 0)
        ip = ntopIPv4(flow.ip)
    var protocol = '*'
    if (flow.protocol > 0)
        protocol = `${flow.protocol}`
    if (Protocols[flow.protocol])
        protocol = Protocols[flow.protocol]
    var ports = '*'
    if (flow.port_min > 0 || flow.ports_max < 65535)
        ports = `${flow.port_min}-${flow.port_max}`
    if (flow.port_min == flow.port_max)
        ports = `${flow.port_min}`
    return `${ip}:${protocol}:${ports}`
}

function parsePortRange(str) {
    const ports = /^(\d+)(?:-(\d+))?$/
    const match = ports.exec(str)
    var [_, port_min,port_max] = match
    port_min = parsePort(parseInt(port_min, 10))
    if (port_max == undefined) {
        port_max = port_min
    } else {
        port_max = parsePort(parseInt(port_max, 10))
    }
    if (!(port_min>=0 && port_max>=0)) throw "Invalid port range specifier"
    if (port_min <= port_max) {
        return [port_min, port_max]
    } else {
        return [port_max, port_min]
    }
}

function ptonIPv4(str) {
    // https://riptutorial.com/regex/example/14146/match-an-ip-address
    const ipv4 = /^(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/
    const match = ipv4.exec(str)
    if (!match) throw "Invalid IPv4 address"
    return ((parseInt(match[1], 10) << 0)
          | (parseInt(match[2], 10) << 8)
          | (parseInt(match[3], 10) << 16)
          | (parseInt(match[4], 10) << 24))
        >>> 0 // https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Unsigned_right_shift
}

function ntopIPv4(ip) {
    const [a,b,c,d] = [(ip >> 0)  & 0xff,
                       (ip >> 8)  & 0xff,
                       (ip >> 16) & 0xff,
                       (ip >> 24) & 0xff]
    return `${a}.${b}.${c}.${d}`
}


function parseProtocol(p) {
    if (typeof(p) != 'number') {
        if (Protocols[p]) return Protocols[p]
        else throw `Unknown protocol: ${p}`
    }
    if (p < 0 || p > 255) throw "Protocol is an 8-bit field"
    return p & 0xff
}

function parsePort(p) {
    if (typeof(p) != 'number' || p < 0 || p > 65535)
        throw "Port is a 16-bit field"
    return p & 0xffff
}

// Label validation
function parseLabel(str) {
    const label = /^[\w_]+$/
    if (!label.exec(str)) throw "Invalid label"
    if (str == "default") throw "Label 'default' is reserved"
    return str
}

function avgBytesPerPacket(packets, bits) {
    // See ../../rush/src/packet::bitlength
    return Math.round((bits/8 - packets*(12+8+4)) / packets)
}
