// Client object
//
// This is a thin wrapper around the JSON API endpoint provided by the
// synthetic network frontend. It handles GET/POST requests and data
// sanitization.
//
const SYNTHETICNET = new SyntheticNetwork({})


// Global state, the sum of all “controls”
// (this is synchronized two-way with the backend and serializes to a
// JSON blob compatible with the backend schema, see sync(), apply())
//
const CONTROLS = new Controls(SYNTHETICNET)

// Synchronize controls initially
CONTROLS.sync()

// Synchronize controls periodically every second
// (but not while we’re dragging a slider, that’s just infuriating)
function syncControlsInterval () {
    var mousedown = false
    document.onmousedown = () => mousedown=true
    document.onmouseup = () => mousedown=false
    return () => {if (!mousedown) CONTROLS.sync()}
}
window.setInterval(syncControlsInterval(), 1*1000)


// Display active traffic profile
const TOP = new Top(SYNTHETICNET)

// Update traffic profile initially
TOP.refresh()

// Refresh traffic profile every five seconds
window.setInterval(() => TOP.refresh(), 5*1000)


// Set up form used to add new flows
const addFlowForm = {
    label: document.querySelector('#flow-label'),
    label_error: document.querySelector('#flow-label-error'),
    label_valid: false,
    expr: document.querySelector('#flow-expr'),
    expr_error: document.querySelector('#flow-expr-error'),
    expr_valid: false,
    button: document.querySelector('#flow-add')
}
addFlowForm.label.addEventListener('input', (e) => {
    try {
        const label = parseLabel(e.target.value)
        if (SYNTHETICNET.flows[label])
            throw "Flow by label already exists"
        addFlowForm.label_error.innerHTML = ''
        addFlowForm.label_valid = true
    } catch (e) {
        addFlowForm.label_error.innerHTML = e
        addFlowForm.label_valid = false
    }
    addFlowForm.button.disabled =
        !(addFlowForm.label_valid && addFlowForm.expr_valid)
})
addFlowForm.expr.addEventListener('input', (e) => {
    try {
        parseFlow(e.target.value)
        addFlowForm.expr_error.innerHTML = ''
        addFlowForm.expr_valid = true
    } catch (e) {
        addFlowForm.expr_error.innerHTML = e
        addFlowForm.expr_valid = false
    }
    addFlowForm.button.disabled =
        !(addFlowForm.label_valid && addFlowForm.expr_valid)
})
addFlowForm.button.addEventListener('click', () => {
    const label = parseLabel(addFlowForm.label.value)
    const flow = parseFlow(addFlowForm.expr.value)
    CONTROLS.addFlow(label, flow)
    addFlowForm.label.value = ''
    addFlowForm.expr.value = ''
    addFlowForm.label_valid = false
    addFlowForm.expr_valid = false
    addFlowForm.button.disabled = true
    CONTROLS.apply()
})
function enterIsClickAdd (e) {
    if (e.key == 'Enter' && !addFlowForm.button.disabled)
        addFlowForm.button.click()
}
[addFlowForm.label, addFlowForm.expr]
  .forEach(input => input.addEventListener('keyup', enterIsClickAdd))


// Controls tree
//
// We make use of a “reader” abstraction. A reader is a closure object
// that knows how to translate the state of an input element (slider,
// checkbox) to a scalar value that has meaning in our data/config schema
// and vice-versa (i.e., set state of input element from scalar value.)
//
// The flowsReader implements this abstraction too, but it’s really a hack as
// there is not a single input element that represents our list of flows.
// Alas, it serves as a proxy that takes care of synchronizing the dynamic
// list of flows with our UI state.
//
function Controls(syntheticnet) {
    this._controls = {
        default_link: {ingress: addQosWidget('ingress', 'default-ingress'),
                       egress: addQosWidget('egress', 'default-egress')},
        flows: {input: [], reader: flowsReader('flows')}
    }

    // Get backend state and update controls
    this.sync = () => {
        return syntheticnet.get().then(() => {
            this.update(syntheticnet.config())
        })
    }
    this.update = (obj, controls) => {
        if (!controls) controls = this._controls
        for (key in obj) {
            const c = controls[key]
            if (typeof(c) != 'object') {
                /* NOP */
            } else if (c.reader) {
                c.reader.sync(c.input, obj[key])
                c.reader.update(c.output, c.input)
            } else {
                this.update(obj[key], c)
            }
        }
    }

    // Push controls state to backend state
    this.apply = () => {
        syntheticnet.update(this.config())
        return syntheticnet.post()
    }
    this.config = (controls) => {
        if (!controls) controls = this._controls
        const conf = {}
        for (key in controls) {
            if (typeof(controls[key]) != 'object') {
                conf[key] = controls[key]
            } else if (controls[key].reader) {
                conf[key] = controls[key].reader.value(controls[key].input)
            } else {
                conf[key] = this.config(controls[key])
            }
        }
        return conf
    }

    // Add/remove flows
    this.addFlow = (label, flow) => {
        const input = this._controls.flows.input
        const reader = this._controls.flows.reader
        reader.add(input, label, flow)
    }
    this.removeFlow = (label) => {
        const input = this._controls.flows.input
        const reader = this._controls.flows.reader
        reader.remove(input, label)
    }
}

// Reusable widget for Quality of Service sliders

function addQosWidget(label, parentId) {
    if (!parentId) parentId = label
    const parent = document.querySelector(`#${parentId}`)
    parent.appendChild(rateControls(`${label}-rate`, "Rate Limit"))
    parent.appendChild(percentControls(`${label}-loss`, "Packet loss"))
    parent.appendChild(delayControls(`${label}-latency`, "Latency"))
    parent.appendChild(delayControls(`${label}-jitter`, "Jitter"))
    parent.appendChild(percentControls(`${label}-jitter-strength`,
`<abbr title="Ratio of packets affected by jitter. 20% means delay
a fifth of all packets by a random jitter amount">Jitter strength</abbr>`))
    parent.appendChild(boolControls(`${label}-jitter-reorder`,
`<abbr title="Packets delayed due to jitter are reordered">
Reorder pkts.</abbr>`))
    const controls = {
        rate: {
            input: document.querySelector(`#${label}-rate`),
            output: document.querySelector(`#${label}-rate-output`),
            reader: rateReader()
        },
        loss: {
            input: document.querySelector(`#${label}-loss`),
            output: document.querySelector(`#${label}-loss-output`),
            reader: percentReader()
        },
        latency: {
            input: document.querySelector(`#${label}-latency`),
            output: document.querySelector(`#${label}-latency-output`),
            reader: delayReader()
        },
        jitter: {
            input: document.querySelector(`#${label}-jitter`),
            output: document.querySelector(`#${label}-jitter-output`),
            reader: delayReader()
        },
        jitter_strength: {
            input: document.querySelector(`#${label}-jitter-strength`),
            output: document.querySelector(`#${label}-jitter-strength-output`),
            reader: percentReader()
        },
        reorder_packets: {
            input: document.querySelector(`#${label}-jitter-reorder`),
            output: document.querySelector(`#${label}-jitter-reorder-output`),
            reader: boolReader()
        }
    }
    Object.values(controls).forEach(c => {
        const input = c.input
        const reader = c.reader
        const output = c.output
        reader.update(output, input)
        input.addEventListener('input', () => reader.update(output, input))
        input.addEventListener('change', () => CONTROLS.apply())
    })
    return controls
}

function rateControls(name, label) {
    if (!label) label = "Rate"
    const tr = document.createElement("tr")
    tr.innerHTML = `
      <td><label for="${name}">${label}</label></td>
      <td><input type="range" id="${name}"
                 min="0" max="200" step="1" value="200"></td>
      <td ><output id="${name}-output" for="${name}"></output></td>
    `
    return tr
}

function percentControls(name, label) {
    if (!label) label = "Percent"
    const tr = document.createElement("tr")
    tr.innerHTML = `
      <td><label for="${name}">${label}</label></td>
      <td><input type="range" id="${name}"
                 min="0" max="100" step="1" value="0"></td>
      <td><output id="${name}-output" for="${name}"></output></td>
    `
    return tr
}

function delayControls(name, label) {
    if (!label) label = "Delay"
    const tr = document.createElement("tr")
    tr.innerHTML = `
      <td><label for="${name}">${label}</label></td>
      <td><input type="range" id="${name}"
                 min="0" max="100" step="1" value="0"></td>
      <td><output id="${name}-output" for="${name}"></output></td>
    `
    return tr
}

function boolControls(name, label) {
    if (!label) label = "Boolean"
    const tr = document.createElement("tr")
    tr.innerHTML = `
      <td><label for="${name}">${label}</label></td>
      <td><input type="checkbox" id="${name}"></td>
    `
    return tr
}

// Helpers for displaying quantities rounded to SI prefixes

function siRound(n, precision) {
    if (!precision) precision = 0
    const round = (n) => Number(n.toFixed(precision))
    const kilo =       1000
    const mega =    1000000
    const giga = 1000000000
    if      (n >= giga) { return [round(n/giga), 'G', giga] }
    else if (n >= mega) { return [round(n/mega), 'M', mega] }
    else if (n >= kilo) { return [round(n/kilo), 'K', kilo] }
    else                { return [n, '', 1] }
}

function siQuantity(n, unit, precision) {
    const [base, prefix] = siRound(n, precision)
    return `${base} ${prefix}${unit}`
}

// (Typical readers follow)

function rateReader() {
    var steps = [0, 50000]
    for (var i=2; i<=200; i++) {steps[i] = steps[i-1]*1.05103}

    function set(val) {
        for (var i=steps.length-1; i>=0; i--)
            if (val >= value({value: i}))
                return i
    }
    function value(input) {
        const [base, _, m] = siRound(steps[input.value], 1)
        return base*m
    }
    function toString(input) {
        return siQuantity(value(input), 'bps', 1)
    }
    function update(output, input) { output.textContent = toString(input) }
    function sync(input, value) { input.value = set(value) }
    return {value:value, update:update, sync:sync}
}

function percentReader() {
    function set(value) { return Math.round(value*100) }
    function value(input) { return parseInt(input.value, 10)/100 }
    function toString(input) { return input.value + '%' }
    function update(output, input) { output.textContent = toString(input) }
    function sync(input, value) { input.value = set(value) }
    return {value:value, update:update, sync:sync}
}

function delayReader() {
    var steps = [0, 1]
    for (var i=2; i<=100; i++) {steps[i] = steps[i-1]*1.07227}

    function set(value) {
        for (var i=steps.length-1; i>=0; i--)
            if (value >= Math.round(steps[i])) return i
    }
    function value(input) { return Math.round(steps[input.value]) }
    function toString(input) { return value(input) + ' ms' }
    function update(output, input) { output.textContent = toString(input) }
    function sync(input, value) { input.value = set(value) }
    return {value:value, update:update, sync:sync}
}

function boolReader() {
    function value(checkbox) { return checkbox.checked }
    function toString(checkbox) { return '' + checkbox.checked }
    function update(output, input) { /* NOP */ }
    function sync(input, value) { input.checked = value }
    return {value:value, update:update, sync:sync}
}


// Reusable widget for per-flow engress/egress QoS slider sets

function addFlowWidget(label, flow, parentId) {
    if (!parentId) parendId = 'flows'
    const flows = document.querySelector(`#${parentId}`)
    const div = document.createElement("div")
    div.id = label
    div.innerHTML = `
      <h2>
        ${label}
        (<tt>${flowString(flow)}</tt>)
        <input id="${label}-remove" type="button" value="Remove">
      </h2>
      <div class="container">
        <div>
          <h3>↓Ingress</h3>
          <table id="${label}-ingress"></table>
        </div>
        <div>
          <h3>↑Egress</h3>
          <table id="${label}-egress"></table>
        </div>
      </div>
    `
    flows.appendChild(div)
    const remove = document.querySelector(`#${label}-remove`)
    remove.addEventListener('click', () => {
        CONTROLS.removeFlow(label)
        CONTROLS.apply()
    })
    return { label: label,
             flow: flow,
             link: {ingress: addQosWidget(`${label}-ingress`),
                    egress: addQosWidget(`${label}-egress`)} }
}

// (This is a hacky pseudo-reader)
function flowsReader(parentId) {
    function add(input, label, flow) {
        input.push(addFlowWidget(label, flow, parentId))
    }
    function remove(input, label) {
        const flows = document.querySelector(`#${parentId}`)
        flows.removeChild(document.querySelector(`#${label}`))
        input.splice(input.findIndex(flow => flow.label == label), 1)
    }
    function clear(input) {
        document.querySelector(`#${parentId}`).innerHTML = ''
        input.length = 0
    }
    function get(input, label) {
        return input.find(fl => fl.label == label)
    }
    function value(input) {
        return input.map(flow => CONTROLS.config(flow))
    }
    function update() { /* NOP */ }
    function sync(input, flows) {
        clear(input)
        flows.forEach(flow => add(input, flow.label, flow.flow))
        flows.forEach(flow => CONTROLS.update(flow, get(input, flow.label)))
    }
    return {value:value, update:update, sync:sync,
            add:add, remove:remove, get:get}
}


// Flow top

function Top(syntheticnet) {
    this.unsorted = false
    this.sort_button = document.querySelector('#top-sort')
    this.sort_button_next_label = "Sort by bps"
    this.sort_button.addEventListener('click', (event) => {
        this.unsorted = !this.unsorted
        var current_label = this.sort_button.innerHTML
        this.sort_button.innerHTML = this.sort_button_next_label
        this.sort_button_next_label = current_label
        this.refresh()
    })
    
    this.ingress = {
        profile: syntheticnet.profiles.ingress,
        table: document.querySelector('#top-ingress'),
        prev: undefined
    }
    this.egress = {
        profile: syntheticnet.profiles.egress,
        table: document.querySelector('#top-egress'),
        prev: undefined
    }
    
    this.refreshTable = async (dir) => {
        const state = this[dir]
        const cur = await state.profile.get()
        if (state.prev) {
            const stats_tree = this.statsTree(cur.diff(state.prev))
            this.renderTable(state.table, stats_tree, dir)
        }
        state.prev = cur
    }
    this.refresh = async () => {
        this.refreshTable('ingress')
        this.refreshTable('egress')
    }
    
    this.statsTree = (stats) => {
        // NB: stats are already sorted by bps (descending)
        const tree = {}
        // Group stats by flow labels
        stats.forEach(flow => {
            const label = SYNTHETICNET.matchFlow(flow.flow) || 'default'
            if (!tree[label]) tree[label] = []
            tree[label].push(flow)
        })
        // Calculate pps and bps totals per flow label and sort groups by
        // total bps (descending)
        const tree_sorted = []
        Object.keys(tree).forEach(label => {
            tree_sorted.push({
                flow_label: label, 
                total_pps: tree[label].map(x => x.pps).reduce((x,y) => x+y),
                total_bps: tree[label].map(x => x.bps).reduce((x,y) => x+y),
                flows: tree[label]
            })
        })
        tree_sorted.sort((x,y) => y.total_bps-x.total_bps)
        // If unsorted mode is active, sort tree by flow order and address
        if (this.unsorted) {
            function flowIndexAscending (x, y) {
                return SYNTHETICNET.flowIndex(x.flow_label)
                     - SYNTHETICNET.flowIndex(y.flow_label)
            }
            tree_sorted.sort(flowIndexAscending)
            function addressPortAscending (x, y) {
                return (x.flow.ip+x.flow.port_min) - (y.flow.ip+y.flow.port_min)
            }
            tree_sorted.forEach(group => group.flows.sort(addressPortAscending))
        }
        
        return tree_sorted
    }
    
    this.renderTable = (table, stats_tree, dir) => {
        table.innerHTML = ''
        stats_tree.forEach(group => {
            table.appendChild(this.renderGroupRow(group, dir))
            group.flows.forEach(flow => {
                table.appendChild(this.renderStatsRow(flow))
            })
        })
        if (stats_tree.length == 0)
            table.appendChild(this.renderCricketsRow())   
    }
    this.renderGroupRow = (group, dir) => {
        const pps = siQuantity(group.total_pps, 'pps', 1)
        const bps = siQuantity(group.total_bps, 'bps', 1)
        var label_a = group.flow_label
        var label_str = group.flow_label
        if (group.flow_label == 'default') {
            label_a = 'default_link'
            label_str = 'default flow'
        }
        const tr = document.createElement("tr")
        tr.className = 'flow_group'
        tr.innerHTML = `
            <td class="quant">${pps}</td>
            <td class="quant">${bps}</td>
            <td><!--empty--></td>
            <td><a href="#${label_a}">${label_str}</a></td>
            <td><a href="#${label_a}" class="plus">&#8613;</a></td>
            <td><a href="#${label_a}" class="minus">&#10515;</a></td>
        `
        const bumps = {
            '.plus':   1.2,
            '.minus':  0.8
        }
        Object.keys(bumps).forEach(bump => {
            tr.querySelector(bump).addEventListener('click', (event) => {
                var qos
                if (group.flow_label == 'default') {
                    qos = SYNTHETICNET.default_link[dir]
                } else {
                    qos = SYNTHETICNET.flows[group.flow_label].link[dir]
                }
                qos.rate(Math.max(group.total_bps * bumps[bump], 50000))
                CONTROLS.update(SYNTHETICNET.config())
                CONTROLS.apply()
                event.preventDefault()
            })
        })
        return tr
    }
    this.renderStatsRow = (flow) => {
        const pps = siQuantity(flow.pps, 'pps', 1)
        const bps = siQuantity(flow.bps, 'bps', 1)
        const bpp = `${flow.bpp} bpp`
        const flow_str = flowString(flow.flow)
        const tr = document.createElement("tr")
        tr.innerHTML = `
            <td class="quant">${pps}</td>
            <td class="quant">${bps}</td>
            <td class="quant">${bpp}</td>
            <td>
                <a href="#add-flow-form" class="flow"><tt>${flow_str}</tt></a>
            </td>
        `
        const flowExpr = document.querySelector('#flow-expr')
        tr.querySelectorAll('.flow').forEach((flow) => {
            flow.addEventListener('click', (event) => {
                flowExpr.value = flow.innerText
                flowExpr.dispatchEvent(new InputEvent('input'))
                flowExpr.select()
                event.preventDefault()
            })
        })
        return tr
    }
    this.renderCricketsRow = () => {
        const tr = document.createElement("tr")
        tr.innerHTML = `<td><i>Crickets…</i></td>`
        return tr
    }
}

