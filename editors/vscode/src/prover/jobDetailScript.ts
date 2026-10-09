/**
 * The job panel's in-webview script (one nonce'd inline script).
 *
 * The host sets the page once; afterwards it sends messages and this script
 * patches the page in place, so focus, open sections, the activity filter and
 * scroll positions survive every update:
 *
 * - `snapshot` {regions, rows, clock, jobId}: full state (after `ready`).
 * - `regions` {regions}: changed regions only, as HTML the host escaped.
 * - `activity` {rows, reset?}: activity rows, built here with textContent.
 * - `clock` {clock}: budget start/deadline/end for the live timer.
 * - `checked` {at}: a refresh found nothing new.
 *
 * Posts `ready` once, and `{command: <data-action>, ...}` for every action.
 */

export function jobDetailScript(rowCap: number): string {
    return `
(function () {
    'use strict';
    const vscode = acquireVsCodeApi();
    const CAP = ${Math.max(1, Math.floor(rowCap))};
    const saved = vscode.getState() || {};
    let filter = ['steps', 'agent', 'all'].indexOf(saved.filter) >= 0 ? saved.filter : 'steps';
    let query = typeof saved.query === 'string' ? saved.query : '';
    const openKeys = new Set(Array.isArray(saved.open) ? saved.open : []);
    const closedKeys = new Set(Array.isArray(saved.closed) ? saved.closed : []);
    let jobId = typeof saved.jobId === 'string' ? saved.jobId : null;
    let clock = null;
    const log = document.getElementById('log');
    const search = document.getElementById('activity-search');
    let lastGroup = null;

    function save() {
        vscode.setState({
            jobId: jobId, filter: filter, query: query,
            open: Array.from(openKeys).slice(-200), closed: Array.from(closedKeys).slice(-200),
        });
    }

    // ── Details open/closed memory ──────────────────────────────────────
    document.addEventListener('toggle', function (e) {
        const d = e.target;
        if (!d || !d.dataset || !d.dataset.key) return;
        if (d.open) { openKeys.add(d.dataset.key); closedKeys.delete(d.dataset.key); }
        else { closedKeys.add(d.dataset.key); openKeys.delete(d.dataset.key); }
        save();
    }, true);

    function restoreDetails(root) {
        root.querySelectorAll('details[data-key]').forEach(function (d) {
            if (openKeys.has(d.dataset.key)) d.open = true;
            else if (closedKeys.has(d.dataset.key)) d.open = false;
        });
    }

    // ── Region patches ──────────────────────────────────────────────────
    function patch(regions) {
        const active = document.activeElement;
        const focusKey = active && active.getAttribute ? active.getAttribute('data-focus-key') : null;
        const scrollY = window.scrollY;
        Object.keys(regions || {}).forEach(function (id) {
            const el = document.getElementById('region-' + id);
            if (el && typeof regions[id] === 'string') {
                el.innerHTML = regions[id];
                restoreDetails(el);
            }
        });
        if (focusKey && (!document.activeElement || document.activeElement === document.body)) {
            const target = document.querySelector('[data-focus-key="' + CSS.escape(focusKey) + '"]');
            if (target) target.focus({ preventScroll: true });
        }
        window.scrollTo(0, scrollY);
        tick();
    }

    // ── Activity ────────────────────────────────────────────────────────
    function clockText(iso) {
        const d = new Date(iso);
        if (isNaN(d.getTime())) return '';
        const p = function (n) { return String(n).padStart(2, '0'); };
        return p(d.getHours()) + ':' + p(d.getMinutes()) + ':' + p(d.getSeconds());
    }
    function shortName(name) {
        const i = name.lastIndexOf('__');
        return i > 0 && i + 2 < name.length ? name.slice(i + 2) : name;
    }
    function catVisible(cat) {
        if (filter === 'steps') return cat !== 'agent' && cat !== 'detail';
        if (filter === 'agent') return cat === 'agent' || cat === 'warn' || cat === 'error';
        return true;
    }
    function matches(row) {
        if (!query) return true;
        const q = query.toLowerCase();
        return row.textContent.toLowerCase().indexOf(q) >= 0 ||
            (row.dataset.obligation || '').toLowerCase().indexOf(q) >= 0;
    }
    function updateGroups() {
        let header = null;
        let any = false;
        Array.prototype.forEach.call(log.children, function (el) {
            if (el.classList.contains('grp')) {
                if (header) header.hidden = !any;
                header = el;
                any = false;
            } else if (el.classList.contains('row') && !el.hidden && catVisible(el.dataset.cat)) {
                any = true;
            }
        });
        if (header) header.hidden = !any;
        const empty = log.querySelector('.empty');
        const visible = Array.prototype.some.call(log.querySelectorAll('.row'), function (r) {
            return !r.hidden && catVisible(r.dataset.cat);
        });
        if (!visible && !empty) {
            const p = document.createElement('div');
            p.className = 'empty';
            p.textContent = log.querySelector('.row') ? 'No matching activity.' : 'No activity yet.';
            log.appendChild(p);
        } else if (visible && empty) {
            empty.remove();
        } else if (empty) {
            empty.textContent = log.querySelector('.row') ? 'No matching activity.' : 'No activity yet.';
        }
    }
    function rowElement(r) {
        const div = document.createElement('div');
        div.className = 'row';
        div.dataset.cat = r.cat;
        div.dataset.seq = String(r.seq);
        if (r.obligation) div.dataset.obligation = r.obligation;
        const t = document.createElement('time');
        t.dateTime = r.ts;
        t.textContent = clockText(r.ts);
        const s = document.createElement('span');
        s.className = 'text';
        if (r.attempt) {
            const a = document.createElement('span');
            a.className = 'att';
            a.textContent = '#' + r.attempt;
            s.appendChild(a);
        }
        s.appendChild(document.createTextNode(r.text));
        div.appendChild(t);
        div.appendChild(s);
        div.hidden = !matches(div);
        return div;
    }
    function groupElement(name) {
        const g = document.createElement('div');
        g.className = 'grp';
        g.textContent = name ? shortName(name) : 'Run';
        if (name) g.title = name;
        return g;
    }
    function appendRows(rows, reset) {
        if (reset) {
            log.textContent = '';
            lastGroup = null;
        }
        const stick = reset || log.scrollTop + log.clientHeight >= log.scrollHeight - 8;
        const frag = document.createDocumentFragment();
        (rows || []).forEach(function (r) {
            const key = r.obligation || '';
            if (key !== lastGroup && (key || lastGroup)) {
                frag.appendChild(groupElement(key));
            }
            lastGroup = key;
            frag.appendChild(rowElement(r));
        });
        log.appendChild(frag);
        let count = log.querySelectorAll('.row').length;
        while (count > CAP && log.firstElementChild) {
            if (log.firstElementChild.classList.contains('row')) count--;
            log.removeChild(log.firstElementChild);
        }
        updateGroups();
        if (stick) log.scrollTop = log.scrollHeight;
    }
    function applyFilter() {
        log.dataset.filter = filter;
        document.querySelectorAll('[data-action="filter"]').forEach(function (b) {
            const on = b.dataset.value === filter;
            b.setAttribute('aria-checked', on ? 'true' : 'false');
            b.tabIndex = on ? 0 : -1;
        });
        log.querySelectorAll('.row').forEach(function (r) { r.hidden = !matches(r); });
        updateGroups();
    }

    // ── Timer ───────────────────────────────────────────────────────────
    function fmt(sec) {
        sec = Math.max(0, Math.round(sec));
        const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
        return h ? h + 'h ' + m + 'm' : m ? m + 'm ' + s + 's' : s + 's';
    }
    function fmtBudget(sec) {
        const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60);
        return h ? h + 'h' + (m ? ' ' + m + 'm' : '') : m ? m + 'm' : sec + 's';
    }
    function tick() {
        const el = document.querySelector('[data-clock]');
        if (!el || !clock || !clock.start) return;
        const start = Date.parse(clock.start);
        if (isNaN(start)) return;
        const now = Date.now();
        let text = '';
        if (clock.running) {
            text = fmt((now - start) / 1000) + ' elapsed';
            const deadline = clock.deadline ? Date.parse(clock.deadline) : NaN;
            if (!isNaN(deadline)) {
                text += deadline > now ? ' · ' + fmt((deadline - now) / 1000) + ' left' : ' · time budget used';
            }
        } else if (clock.end && !isNaN(Date.parse(clock.end))) {
            text = 'Took ' + fmt((Date.parse(clock.end) - start) / 1000) +
                (clock.budgetSeconds ? ' of ' + fmtBudget(clock.budgetSeconds) : '');
        } else if (clock.budgetSeconds) {
            text = 'Time budget ' + fmtBudget(clock.budgetSeconds);
        }
        el.textContent = text;
    }
    setInterval(function () { if (clock && clock.running) tick(); }, 1000);

    // ── Actions ─────────────────────────────────────────────────────────
    document.addEventListener('click', function (e) {
        const el = e.target && e.target.closest ? e.target.closest('[data-action]') : null;
        if (!el) return;
        const action = el.dataset.action;
        if (action === 'filter') {
            filter = el.dataset.value;
            applyFilter();
            save();
            return;
        }
        if (action === 'show-activity') {
            query = el.dataset.obligation || '';
            search.value = query;
            filter = 'all';
            applyFilter();
            save();
            document.getElementById('activity').scrollIntoView({ block: 'start' });
            return;
        }
        const message = { command: action };
        if (el.dataset.artifact) message.artifactId = el.dataset.artifact;
        if (el.dataset.line) message.line = Number(el.dataset.line);
        if (el.dataset.obligation) message.obligation = el.dataset.obligation;
        vscode.postMessage(message);
    });
    document.addEventListener('keydown', function (e) {
        const el = e.target;
        if (!el || !el.dataset || el.dataset.action !== 'filter') return;
        if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft' && e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
        const radios = Array.prototype.slice.call(document.querySelectorAll('[data-action="filter"]'));
        const i = radios.indexOf(el);
        const next = radios[(i + (e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : radios.length - 1)) % radios.length];
        filter = next.dataset.value;
        applyFilter();
        save();
        next.focus();
        e.preventDefault();
    });
    search.addEventListener('input', function () {
        query = search.value.trim();
        applyFilter();
        save();
    });

    // ── Host messages ───────────────────────────────────────────────────
    window.addEventListener('message', function (event) {
        const m = event.data;
        if (!m || typeof m.command !== 'string') return;
        switch (m.command) {
            case 'snapshot':
                if (typeof m.jobId === 'string') jobId = m.jobId;
                clock = m.clock || null;
                patch(m.regions);
                appendRows(m.rows, true);
                document.getElementById('checked').textContent = '';
                save();
                break;
            case 'regions':
                patch(m.regions);
                break;
            case 'activity':
                appendRows(m.rows, Boolean(m.reset));
                break;
            case 'clock':
                clock = m.clock || null;
                tick();
                break;
            case 'checked':
                document.getElementById('checked').textContent =
                    'Checked at ' + m.at + ' — no changes.';
                break;
        }
    });

    // Initial page state: restore the saved filter and sections, then ask
    // the host for the current data.
    search.value = query;
    restoreDetails(document);
    lastGroup = (function () {
        const rows = log.querySelectorAll('.row');
        return rows.length ? rows[rows.length - 1].dataset.obligation || '' : null;
    })();
    applyFilter();
    vscode.postMessage({ command: 'ready' });
})();
`;
}
