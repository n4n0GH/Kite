.pragma library
// Tree-style tabs: the rules, without QML (tested with node: tabtree.test.js).
//
// A tab row is {id, parent, collapsed, pinned, pane, group}; `parent` is a
// tab id or -1. A tab's descendants are the tabs right after it, in order, so
// the list of tabs and the links between them are all there is to the tree.

// What the rows mean: for each one, in order,
//   parent:    the link, or -1 if it can't hold (the parent isn't above it as
//              an ancestor, is pinned, or is in another pane or group)
//   depth:     how many ancestors it has
//   hidden:    under a collapsed ancestor
//   count:     how many descendants it has
//   collapsed: whether it is collapsed (only tabs with descendants can be)
function compute(rows) {
    const out = rows.map(() => ({parent: -1, depth: 0, hidden: false, count: 0, collapsed: false}))
    let path = []   // indexes of the ancestors of the next row, and of the row before it
    for (let i = 0; i < rows.length; ++i) {
        const r = rows[i]
        const at = r.pinned ? -1 : path.findIndex(j => rows[j].id === r.parent)
        const up = at >= 0 ? rows[path[at]] : null
        if (up && !up.pinned && up.pane === r.pane && up.group === r.group) {
            path = path.slice(0, at + 1)
            const above = path[at]
            out[i].parent = up.id
            out[i].depth = path.length
            out[i].hidden = out[above].hidden || !!up.collapsed
            for (const j of path) out[j].count++
        } else {
            path = []
        }
        path.push(i)
    }
    for (let i = 0; i < rows.length; ++i) out[i].collapsed = !!rows[i].collapsed && out[i].count > 0
    return out
}

// The parent of a tab put at `index` without one in mind: it joins the level
// of the tab it pushes down, so that tab's own link stays intact.
function parentAt(rows, index) {
    return index >= 0 && index < rows.length && rows[index].parent >= 0 ? rows[index].parent : -1
}

// The parent of the tabs first..last (a tab and its descendants), dropped
// where they are, at `wanted` depth: as near as the tabs around them allow.
//  * no deeper than a child of the tab above,
//  * no shallower than the tab below, which would otherwise lose its parent,
//  * never out of sight, under a collapsed tab: then it goes after that tab,
//  * and only among tabs of its own group: the others' levels don't apply.
function dropParent(rows, first, last, wanted) {
    if (rows[first].pinned) return -1
    const rest = rows.slice(0, first).concat(rows.slice(last + 1))
    const tree = compute(rest)
    let above = first - 1
    if (above < 0) return -1
    const indexOf = id => rest.findIndex(r => r.id === id)
    while (tree[above].hidden) above = indexOf(tree[above].parent)
    if (rest[above].pinned || rest[above].group !== rows[first].group) return -1
    const deepest = tree[above].collapsed ? tree[above].depth : tree[above].depth + 1
    const below = first < rest.length && rest[first].group === rows[first].group ? tree[first].depth : 0
    const depth = Math.max(below, Math.min(deepest, wanted))
    if (depth === 0) return -1
    // The ancestor (or the tab above itself) one level up from there.
    let up = above
    while (tree[up].depth > depth - 1) up = indexOf(tree[up].parent)
    return rest[up].id
}

// A step of the tab at `first` (with its descendants up to `last`) through its pane,
// by keyboard (`step` -1 or 1). What it passes counts as one spot: a tab with its
// descendants, or a whole group. It stays among its siblings, in its pane, and
// pinned or unpinned as it is. Answers {to}, where its first row goes once it is
// out of the way of what it passes; {leaveGroup: true} when at its group's edge,
// which is the step; or null when it can't go on.
function stepTarget(rows, first, last, step) {
    const tree = compute(rows)
    const me = rows[first]
    const samePane = i => rows[i].pane === me.pane
    let n = step < 0 ? first - 1 : last + 1
    while (n >= 0 && n < rows.length && !samePane(n)) n += step
    if (n < 0 || n >= rows.length || rows[n].pinned !== me.pinned) return null
    if (rows[n].group !== me.group) {
        if (me.group) return {leaveGroup: true}
        // Past the whole group.
        const g = rows[n].group
        while (n + step >= 0 && n + step < rows.length && rows[n + step].group === g) n += step
        return {to: step < 0 ? n : n - (last - first)}
    }
    const depth = tree[first].depth
    if (step < 0) {
        // Up to the sibling it is in the descendants of, if any.
        while (tree[n].depth > depth) n = rows.findIndex(r => r.id === tree[n].parent)
        return tree[n].depth < depth ? null : {to: n}
    }
    if (tree[n].depth < depth) return null
    return {to: n + tree[n].count - (last - first)}
}

// Of the ids, those with none of the others above them in the tree: acting
// on these with their descendants covers them all, once each.
function roots(rows, ids) {
    return ids.filter(id => !ancestors(rows, id).some(up => ids.includes(up)))
}

// The ids of the tab's ancestors, nearest first.
function ancestors(rows, id) {
    const out = []
    let at = rows.findIndex(r => r.id === id)
    while (at >= 0 && rows[at].parent >= 0) {
        const parent = rows[at].parent
        out.push(parent)
        at = rows.findIndex(r => r.id === parent)
    }
    return out
}
