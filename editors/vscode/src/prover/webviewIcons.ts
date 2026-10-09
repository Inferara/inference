/**
 * Small 16px line icons for the job panel, drawn in the style of VS Code's
 * codicons. Inline SVG in `currentColor`: no packaged font, no CSP change,
 * and every icon follows the theme. Always decorative (`aria-hidden`) —
 * meaning is carried by adjacent text.
 */

const PATHS = {
    pass: '<circle cx="8" cy="8" r="6.25"/><path d="M5.2 8.3l1.9 1.9 3.8-4.1"/>',
    error: '<circle cx="8" cy="8" r="6.25"/><path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4"/>',
    warning: '<path d="M8 1.9l6.6 11.7H1.4z"/><path d="M8 6.2v3.5"/><circle cx="8" cy="11.7" r=".7" fill="currentColor" stroke="none"/>',
    refuted: '<path d="M8 1.6L14.4 8 8 14.4 1.6 8z"/><path d="M6.2 6.2l3.6 3.6M9.8 6.2l-3.6 3.6"/>',
    running: '<path d="M13.2 6.4A5.4 5.4 0 0 0 3.3 5.2"/><path d="M3.1 2.6v2.8h2.8"/><path d="M2.8 9.6a5.4 5.4 0 0 0 9.9 1.2"/><path d="M12.9 13.4v-2.8h-2.8"/>',
    loading: '<path d="M8 2.2a5.8 5.8 0 1 0 5.8 5.8"/>',
    waiting: '<circle cx="8" cy="8" r="6.25"/><path d="M8 4.6V8l2.4 1.6"/>',
    verifying: '<path d="M8 1.7l5.2 2v4.1c0 3.1-2.2 5.4-5.2 6.5-3-1.1-5.2-3.4-5.2-6.5V3.7z"/><path d="M5.7 8.1l1.6 1.6 3-3.2"/>',
    canceled: '<circle cx="8" cy="8" r="6.25"/><path d="M3.6 12.4l8.8-8.8"/>',
    todo: '<circle cx="8" cy="8" r="4.5"/>',
    dot: '<circle cx="8" cy="8" r="2.2" fill="currentColor" stroke="none"/>',
    chevron: '<path d="M6 3.5L10.5 8 6 12.5"/>',
    linkExternal: '<path d="M9.5 2.5h4v4"/><path d="M13.5 2.5l-6 6"/><path d="M12 9.5v3.4c0 .3-.3.6-.6.6H3.1c-.3 0-.6-.3-.6-.6V4.6c0-.3.3-.6.6-.6h3.4"/>',
    file: '<path d="M9.3 1.8H4.1c-.4 0-.7.3-.7.7v11c0 .4.3.7.7.7h7.8c.4 0 .7-.3.7-.7V5.1z"/><path d="M9.3 1.8v3.3h3.3"/>',
    diff: '<rect x="2" y="2.5" width="5" height="11" rx=".6"/><rect x="9" y="2.5" width="5" height="11" rx=".6"/><path d="M3.6 6h1.8M4.5 5.1v1.8M10.6 8h1.8"/>',
    copy: '<rect x="5.3" y="5.3" width="8.2" height="8.2" rx=".8"/><path d="M10.7 5.3v-2c0-.4-.3-.8-.8-.8H3.3c-.4 0-.8.3-.8.8v6.6c0 .4.3.8.8.8h2"/>',
    stop: '<circle cx="8" cy="8" r="6.25"/><rect x="5.8" y="5.8" width="4.4" height="4.4" rx=".5"/>',
    refresh: '<path d="M13 8a5 5 0 1 1-1.6-3.7"/><path d="M11.8 1.8v2.8H9"/>',
    search: '<circle cx="6.8" cy="6.8" r="4.3"/><path d="M10 10l4 4"/>',
    download: '<path d="M8 2.5V10"/><path d="M4.8 7L8 10.2 11.2 7"/><path d="M2.8 13.5h10.4"/>',
    rerun: '<path d="M12.9 9.4A5.2 5.2 0 1 1 11.7 4"/><path d="M12.4 1.9v2.8H9.6"/><path d="M6.8 6.2v3.6L9.6 8z"/>',
    goto: '<path d="M2.5 8h8.5"/><path d="M8 4.8L11.2 8 8 11.2"/><path d="M13.5 3v10"/>',
    log: '<path d="M3 3.5h10M3 6.5h10M3 9.5h7M3 12.5h5"/>',
} as const;

export type IconName = keyof typeof PATHS;

/** An inline, decorative icon. */
export function icon(name: IconName, extraClass = ''): string {
    const cls = extraClass ? `icon ${extraClass}` : 'icon';
    return `<svg class="${cls}" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">${PATHS[name]}</svg>`;
}
