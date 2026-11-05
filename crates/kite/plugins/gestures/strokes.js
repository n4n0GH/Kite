.pragma library
// Mouse gestures: from pointer positions to a stroke such as "DR" (down, then
// right). Without QML (tested with node: strokes.test.js).

const STEP = 24        // px to travel before a direction counts
const CLEAR = 1.5      // how much the main axis must lead: diagonals say nothing
const LONGEST = 12     // directions in a stroke that can be given an action
const FORGIVING = 4    // directions from which a stroke needn't be exact (match)

function tracker(x, y) {
    return {x, y, stroke: ""}
}

// The pointer moved to x, y; answers the stroke so far.
function track(t, x, y) {
    const dx = x - t.x, dy = y - t.y
    const ax = Math.abs(dx), ay = Math.abs(dy)
    if (Math.max(ax, ay) < STEP) return t.stroke
    t.x = x
    t.y = y
    if (Math.max(ax, ay) < CLEAR * Math.min(ax, ay)) return t.stroke
    const dir = ax > ay ? (dx < 0 ? "L" : "R") : (dy < 0 ? "U" : "D")
    if (!t.stroke.endsWith(dir)) t.stroke += dir
    return t.stroke
}

// Can it be given an action? (What track() gives: never a direction twice in a row.)
function assignable(stroke) {
    return stroke.length > 0 && stroke.length <= LONGEST && /^[LRUD]+$/.test(stroke) && !/(.)\1/.test(stroke)
}

// How many directions to add, drop or change to get from one stroke to the other.
function distance(a, b) {
    let row = Array.from({length: b.length + 1}, (_, i) => i)
    for (let i = 1; i <= a.length; ++i) {
        const next = [i]
        for (let j = 1; j <= b.length; ++j)
            next[j] = Math.min(row[j] + 1, next[j - 1] + 1, row[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1))
        row = next
    }
    return row[b.length]
}

// The stroke of the map meant by what was drawn, or "". Hands aren't exact in
// long shapes (a circle may or may not get its last bit in), so from FORGIVING
// directions on, one off will do, if it can only mean one of them. Short strokes
// are too near each other for that.
function match(map, drawn) {
    if (Object.prototype.hasOwnProperty.call(map, drawn)) return drawn
    if (drawn.length < FORGIVING) return ""
    const near = Object.keys(map).filter(s => s.length >= FORGIVING && distance(s, drawn) === 1)
    return near.length === 1 ? near[0] : ""
}

// The strokes of a map for showing: the short, simple ones first.
function sorted(map) {
    return Object.keys(map).sort((a, b) => a.length - b.length || a.localeCompare(b))
}

function arrows(stroke) {
    const of = {L: "←", R: "→", U: "↑", D: "↓"}
    return stroke.split("").map(d => of[d]).join("")
}
