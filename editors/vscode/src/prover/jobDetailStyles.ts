/**
 * Job panel stylesheet. Every colour comes from the VS Code theme
 * (`--vscode-*`), so the panel follows light, dark and high-contrast themes;
 * tone colours are never the only signal (each has a label or icon shape).
 */
export const JOB_DETAIL_STYLES = `
:root {
    --ok: var(--vscode-testing-iconPassed, #73c991);
    --err: var(--vscode-testing-iconFailed, var(--vscode-errorForeground, #f14c4c));
    --warn: var(--vscode-editorWarning-foreground, #cca700);
    --refuted: var(--vscode-charts-purple, #b180d7);
    --active: var(--vscode-progressBar-background, #0e70c0);
    --muted: var(--vscode-descriptionForeground, #8b8b8b);
    --border: var(--vscode-widget-border, var(--vscode-panel-border, rgba(128, 128, 128, 0.35)));
    --code-bg: var(--vscode-textCodeBlock-background, rgba(128, 128, 128, 0.12));
    --focus: var(--vscode-focusBorder, #007fd4);
    --tone: var(--muted);
}
.tone-ok { --tone: var(--ok); }
.tone-err { --tone: var(--err); }
.tone-warn { --tone: var(--warn); }
.tone-refuted { --tone: var(--refuted); }
.tone-active { --tone: var(--active); }
.tone-muted { --tone: var(--muted); }

body {
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size, 13px);
    color: var(--vscode-foreground);
    line-height: 1.5;
    margin: 0;
    padding: 16px 24px 40px;
}
main { max-width: 980px; }
code, pre { font-family: var(--vscode-editor-font-family); font-size: 0.95em; }
h1, h2, h3 { font-weight: 600; margin: 0; }
h1 { font-size: 1.4em; line-height: 1.3; overflow-wrap: anywhere; }
h2 { font-size: 1.05em; }
h3 { font-size: 1em; margin: 14px 0 6px; }
p { margin: 0; }
.muted { color: var(--muted); }
.sr-only {
    position: absolute; width: 1px; height: 1px; overflow: hidden;
    clip: rect(0 0 0 0); clip-path: inset(50%); white-space: nowrap;
}
.icon { width: 16px; height: 16px; flex: none; vertical-align: -3px; }
.spin { animation: spin 1.2s linear infinite; transform-origin: 50% 50%; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
:focus-visible { outline: 1px solid var(--focus); outline-offset: 2px; }

/* Buttons follow VS Code's button tokens. */
button {
    font: inherit; line-height: 18px;
    display: inline-flex; align-items: center; gap: 6px;
    padding: 4px 12px; border-radius: 2px; cursor: pointer;
    border: 1px solid var(--vscode-button-border, transparent);
    background: var(--vscode-button-background);
    color: var(--vscode-button-foreground);
}
button:hover { background: var(--vscode-button-hoverBackground); }
button .icon { width: 14px; height: 14px; vertical-align: 0; }
button.secondary {
    background: var(--vscode-button-secondaryBackground, transparent);
    color: var(--vscode-button-secondaryForeground, var(--vscode-foreground));
}
button.secondary:hover { background: var(--vscode-button-secondaryHoverBackground, var(--vscode-toolbar-hoverBackground)); }
button.danger {
    background: transparent; color: var(--vscode-foreground);
    border-color: color-mix(in srgb, var(--err) 65%, transparent);
}
button.danger .icon { color: var(--err); }
button.danger:hover { background: color-mix(in srgb, var(--err) 14%, transparent); }
button.link {
    background: none; border-color: transparent; padding: 2px 4px;
    color: var(--vscode-textLink-foreground);
}
button.link:hover { background: none; color: var(--vscode-textLink-activeForeground); text-decoration: underline; }
.actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
.actions.small { gap: 4px 12px; margin-top: 8px; }

.banner {
    display: flex; gap: 8px; align-items: flex-start;
    padding: 8px 12px; margin: 0 0 14px; border-radius: 4px;
    border: 1px solid var(--tone);
    background: color-mix(in srgb, var(--tone) 10%, transparent);
}
.banner .icon { color: var(--tone); margin-top: 2px; }

.header {
    display: flex; justify-content: space-between; align-items: flex-start;
    flex-wrap: wrap; gap: 8px 16px;
}
.title { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
.title > .icon { width: 20px; height: 20px; color: var(--tone); }
.subtitle { color: var(--muted); margin: 2px 0 0 28px; }
.header-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.badge {
    display: inline-flex; align-items: center; white-space: nowrap;
    padding: 0 8px; border-radius: 10px; font-size: 0.85em; font-weight: 600; line-height: 20px;
    color: var(--vscode-foreground);
    background: color-mix(in srgb, var(--tone) 16%, transparent);
    border: 1px solid color-mix(in srgb, var(--tone) 60%, transparent);
}
.live { display: inline-flex; align-items: center; gap: 2px; color: var(--muted); font-size: 0.9em; }
.live .icon { color: var(--active); }

.steps {
    list-style: none; padding: 0; margin: 14px 0 16px;
    display: flex; flex-wrap: wrap; align-items: center; gap: 4px 0;
}
.step { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
.step .sep { display: inline-flex; margin: 0 6px; opacity: 0.6; }
.step .sep .icon { width: 12px; height: 12px; }
.step.done { color: var(--vscode-foreground); }
.step.done > .icon { color: var(--ok); }
.step.active { color: var(--vscode-foreground); font-weight: 600; }
.step.active > .icon { color: var(--active); }
.step.unknown > .icon { opacity: 0.7; }
.step.stopped { color: var(--vscode-foreground); }
.step.stopped > .icon { color: var(--tone); }
.step.final { color: var(--vscode-foreground); font-weight: 600; }
.step.final > .icon { color: var(--tone); }

.verdict {
    margin: 0 0 20px; padding: 12px 16px; border-radius: 4px;
    border: 1px solid var(--border); border-left: 3px solid var(--tone);
    background: color-mix(in srgb, var(--tone) 7%, transparent);
}
.verdict h2 { display: flex; align-items: center; gap: 8px; font-size: 1.1em; margin-bottom: 4px; }
.verdict h2 .icon { color: var(--tone); }
.verdict .caveats { margin: 8px 0 0; padding-left: 20px; }
.verdict .caveats li + li { margin-top: 2px; }
pre.excerpt {
    margin: 6px 0 0; padding: 8px 12px; border-radius: 3px;
    background: var(--code-bg); white-space: pre-wrap; overflow-wrap: anywhere;
}

.progress { margin-bottom: 10px; }
.progress-head { display: flex; align-items: baseline; gap: 6px 14px; flex-wrap: wrap; }
.progress-head .count { font-variant-numeric: tabular-nums; }
.progress-head .failed-note { color: var(--err); }
.clock { margin-left: auto; color: var(--muted); font-variant-numeric: tabular-nums; }
.bar {
    display: flex; height: 6px; margin: 8px 0 14px; border-radius: 3px; overflow: hidden;
    background: color-mix(in srgb, var(--muted) 22%, transparent);
}
.bar .closed { background: var(--ok); }
.bar .failed { background: var(--err); }
.bar.loading { opacity: 0.6; }

.obligations { border: 1px solid var(--border); border-radius: 4px; overflow: hidden; }
.empty { color: var(--muted); padding: 8px 0; }
details > summary { list-style: none; cursor: pointer; }
details > summary::-webkit-details-marker { display: none; }
.chev { display: inline-flex; transition: transform 0.1s; color: var(--muted); }
details[open] > summary .chev { transform: rotate(90deg); }
@media (prefers-reduced-motion: reduce) { .chev { transition: none; } }
details.group > summary {
    display: flex; align-items: center; gap: 6px; padding: 6px 10px;
    font-weight: 600; background: color-mix(in srgb, var(--muted) 8%, transparent);
    border-top: 1px solid var(--border);
}
details.group:first-child > summary { border-top: none; }
details.obl { border-top: 1px solid var(--border); }
.obligations > details.obl:first-child { border-top: none; }
details.obl > summary {
    display: grid; grid-template-columns: 16px minmax(0, 1fr) auto 12px;
    align-items: center; gap: 10px; padding: 6px 10px;
}
details.obl > summary:hover { background: var(--vscode-list-hoverBackground); }
.obl-icon { display: inline-flex; color: var(--tone); }
.obl-main { display: flex; align-items: center; gap: 8px; min-width: 0; }
.obl-name { font-family: var(--vscode-editor-font-family); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.chip {
    flex: none; font-size: 0.85em; color: var(--muted); padding: 0 6px; border-radius: 8px;
    border: 1px solid var(--border);
}
.obl-meta { display: flex; gap: 12px; color: var(--muted); white-space: nowrap; font-variant-numeric: tabular-nums; }
.obl-status { color: var(--vscode-foreground); }
.obl-body { padding: 4px 14px 12px 36px; }
.facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 2px 14px; margin: 4px 0 0; }
.facts dt { color: var(--muted); }
.facts dd { margin: 0; overflow-wrap: anywhere; }
.obl-error { display: flex; gap: 6px; margin-top: 8px; color: var(--vscode-foreground); }
.obl-error .icon { color: var(--err); margin-top: 2px; }
.goal-label { margin-top: 10px; color: var(--muted); font-size: 0.85em; }
pre.goal {
    margin: 2px 0 0; padding: 8px 12px; border-radius: 3px; background: var(--code-bg);
    white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.45;
}
@media (max-width: 560px) {
    details.obl > summary { grid-template-columns: 16px minmax(0, 1fr) 12px; }
    .obl-meta { grid-column: 2 / 3; }
    .obl-body { padding-left: 14px; }
}

details.section { margin-top: 16px; border-top: 1px solid var(--border); padding-top: 10px; }
details.section > summary { display: flex; align-items: center; gap: 6px; }
details.section > summary h2 { display: inline; }
.section-body { padding: 8px 0 0 20px; }
.checks { list-style: none; padding: 0; margin: 8px 0 0; }
.checks li { display: flex; gap: 8px; align-items: flex-start; padding: 2px 0; }
.checks li .icon { margin-top: 2px; }
.checks .pass .icon { color: var(--ok); }
.checks .fail .icon { color: var(--err); }
.checks .warn .icon { color: var(--warn); }
.checks .unknown .icon { color: var(--muted); }
details.assume { margin-top: 6px; }
details.assume > summary { display: flex; align-items: center; gap: 6px; }
table { border-collapse: collapse; width: 100%; margin: 6px 0; }
th, td { text-align: left; padding: 4px 12px 4px 0; vertical-align: top; }
th { color: var(--muted); font-weight: 600; border-bottom: 1px solid var(--border); }
.files { list-style: none; padding: 0; margin: 4px 0 0; }
.files li {
    display: grid; grid-template-columns: 16px minmax(0, 1fr) auto; gap: 10px; align-items: center;
    padding: 4px 0; border-bottom: 1px solid var(--border);
}
.files li:last-child { border-bottom: none; }
.file-name { display: flex; flex-wrap: wrap; gap: 0 10px; min-width: 0; }
.file-actions { display: flex; gap: 4px; }
dl.tech { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 4px 16px; margin: 4px 0 0; }
dl.tech dt { color: var(--muted); }
dl.tech dd { margin: 0; overflow-wrap: anywhere; }

.activity { margin-top: 24px; }
.activity-head { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px; }
.activity-tools { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.segmented { display: inline-flex; border: 1px solid var(--border); border-radius: 3px; overflow: hidden; }
.segmented button {
    background: transparent; color: var(--vscode-foreground); border: none; border-radius: 0; padding: 2px 10px;
}
.segmented button + button { border-left: 1px solid var(--border); }
.segmented button[aria-checked="true"] {
    background: var(--vscode-button-secondaryBackground, color-mix(in srgb, var(--muted) 25%, transparent));
    color: var(--vscode-button-secondaryForeground, var(--vscode-foreground));
}
.search {
    display: inline-flex; align-items: center; gap: 6px; padding: 2px 6px; border-radius: 2px;
    border: 1px solid var(--vscode-input-border, var(--border));
    background: var(--vscode-input-background); color: var(--vscode-input-foreground);
}
.search:focus-within { outline: 1px solid var(--focus); outline-offset: -1px; }
.search .icon { color: var(--muted); width: 14px; height: 14px; }
.search input { border: none; outline: none; background: transparent; color: inherit; font: inherit; width: 14em; }
.log {
    margin-top: 8px; padding: 6px 0; border-radius: 4px; max-height: 28em; overflow: auto;
    font-family: var(--vscode-editor-font-family); font-size: var(--vscode-editor-font-size, 12px); line-height: 1.6;
    background: var(--code-bg);
}
.log .row { display: grid; grid-template-columns: 5.6em minmax(0, 1fr); gap: 10px; padding: 0 12px; }
.log .row time { color: var(--muted); font-variant-numeric: tabular-nums; }
.log .row .text { white-space: pre-wrap; overflow-wrap: anywhere; }
.log .row .att { color: var(--muted); margin-right: 6px; }
.log .row[data-cat="agent"] .text { color: color-mix(in srgb, var(--vscode-foreground) 82%, transparent); }
.log .row[data-cat="warn"] .text { color: var(--warn); }
.log .row[data-cat="error"] .text { color: var(--err); }
.log .grp {
    padding: 8px 12px 2px; font-family: var(--vscode-font-family); font-size: 0.92em;
    font-weight: 600; color: var(--muted);
}
.log .empty { padding: 0 12px; }
.log[data-filter="steps"] .row[data-cat="agent"],
.log[data-filter="steps"] .row[data-cat="detail"],
.log[data-filter="agent"] .row[data-cat="step"],
.log[data-filter="agent"] .row[data-cat="detail"],
.log [hidden] { display: none; }
.footer { margin-top: 16px; min-height: 1.5em; color: var(--muted); }
.skeleton {
    height: 12px; margin: 10px; border-radius: 3px;
    background: color-mix(in srgb, var(--muted) 18%, transparent);
}

body.vscode-high-contrast, body.vscode-high-contrast-light {
    --border: var(--vscode-contrastBorder, currentColor);
}
body.vscode-high-contrast .log, body.vscode-high-contrast-light .log { border: 1px solid var(--border); }
body.vscode-high-contrast .badge, body.vscode-high-contrast-light .badge,
body.vscode-high-contrast .verdict, body.vscode-high-contrast-light .verdict,
body.vscode-high-contrast .banner, body.vscode-high-contrast-light .banner {
    background: transparent;
    border-color: var(--vscode-contrastBorder, currentColor);
}
body.vscode-high-contrast .verdict, body.vscode-high-contrast-light .verdict { border-left: 3px solid var(--tone); }
body.vscode-high-contrast .bar, body.vscode-high-contrast-light .bar { outline: 1px solid var(--vscode-contrastBorder, currentColor); }
body.vscode-high-contrast :focus-visible, body.vscode-high-contrast-light :focus-visible {
    outline: 1px solid var(--vscode-contrastActiveBorder, var(--focus));
}
`;
