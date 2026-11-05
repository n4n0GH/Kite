.pragma library
// Fuzzy matching for the command bars (CommandBar.qml), like KDE's: the letters
// of each word typed appear in order in the text, and matches at word starts
// and in runs count for more. Tested with node: fuzzy.test.js.

function isWordStart(text, i) {
    if (i === 0) return true
    const prev = text[i - 1], c = text[i]
    if (!/[\p{L}\p{N}]/u.test(prev)) return true
    return prev === prev.toLowerCase() && prev !== prev.toUpperCase() && c !== c.toLowerCase()
}

// One word against the text: {score, positions}, or null if it doesn't match.
function matchWord(word, text) {
    const w = word.toLowerCase(), t = text.toLowerCase()
    if (!w.length) return {score: 0, positions: []}
    // best[i][j]: the best score with letter i of the word at place j of the text.
    const best = [], from = []
    for (let i = 0; i < w.length; ++i) { best.push(new Array(t.length).fill(-Infinity)); from.push(new Array(t.length).fill(-1)) }
    for (let i = 0; i < w.length; ++i) {
        for (let j = i; j < t.length; ++j) {
            if (t[j] !== w[i]) continue
            const here = 1 + (isWordStart(text, j) ? 6 : 0)
            if (i === 0) { best[i][j] = here - j * 0.05; continue }
            for (let k = i - 1; k < j; ++k) {
                if (best[i - 1][k] === -Infinity) continue
                const run = k === j - 1 ? 8 : -Math.min(3, (j - k) * 0.1)
                const s = best[i - 1][k] + here + run
                if (s > best[i][j]) { best[i][j] = s; from[i][j] = k }
            }
        }
    }
    let end = -1, score = -Infinity
    for (let j = 0; j < t.length; ++j) if (best[w.length - 1][j] > score) { score = best[w.length - 1][j]; end = j }
    if (end < 0) return null
    const positions = []
    for (let i = w.length - 1, j = end; i >= 0; j = from[i][j], --i) positions.unshift(j)
    return {score, positions}
}

// All the words typed, in any order: {score, positions}, or null.
function match(query, text) {
    const words = query.trim().split(/\s+/).filter(w => w)
    let score = 0
    const positions = new Set()
    for (const word of words) {
        const m = matchWord(word, text)
        if (!m) return null
        score += m.score
        for (const p of m.positions) positions.add(p)
    }
    return {score, positions: Array.from(positions).sort((a, b) => a - b)}
}

// The text as markup for a Label: matched letters in `hit`, the first
// `dimLength` characters in `dim`.
function markup(text, positions, hit, dim, dimLength) {
    const on = new Set(positions)
    const esc = c => c === "&" ? "&amp;" : c === "<" ? "&lt;" : c === ">" ? "&gt;" : c === '"' ? "&quot;" : c
    let out = "", open = ""
    for (let i = 0; i < text.length; ++i) {
        const want = on.has(i) ? hit : i < dimLength ? dim : ""
        if (want !== open) {
            if (open) out += "</font>"
            if (want) out += '<font color="' + want + '">'
            open = want
        }
        out += esc(text[i])
    }
    return out + (open ? "</font>" : "")
}

// A key sequence as shown ("Ctrl+Shift+P"), as its keys ("+" can be one).
function keycaps(text) {
    const keys = []
    for (const part of text.split(/\+(?=.)/)) keys.push(part)
    return keys
}
