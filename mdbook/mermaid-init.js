// Theme-aware Mermaid initialisation for mdBook.
//
// Why this replaces `mdbook-mermaid`'s stock init: that one detects the theme from
// the `coal`/`navy`/`ayu` class on <html>, which misses mdBook's OS-following
// theme, and it does not cope with mdBook changing the theme at runtime.
//
// Two details matter:
//   * `mermaid.min.js` registers its own DOMContentLoaded handler that auto-renders
//     with the *default* (light) theme. We must call `initialize({startOnLoad:false})`
//     **synchronously** (this script runs at end of <body>, after book.js has already
//     applied the theme class) so Mermaid's auto-render is disabled; then we render
//     ourselves. Deferring `initialize` lets Mermaid render light first.
//   * Mermaid cannot restyle already-drawn diagrams, so a light/dark flip reloads.
(() => {
    const DARK_CLASSES = ['coal', 'navy', 'ayu'];
    const LIGHT_CLASSES = ['light', 'rust'];
    const html = document.documentElement;

    const prefersDark = () =>
        window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;

    const rgbOf = (el) => {
        const bg = el ? getComputedStyle(el).backgroundColor : '';
        const m = bg && bg.match(/-?\d*\.?\d+/g);
        // No colour, too few components, or fully transparent → not usable.
        if (!m || m.length < 3 || (m.length >= 4 && Number(m[3]) === 0)) return null;
        return m.slice(0, 3).map(Number);
    };

    const isDark = () => {
        // Prefer the known mdBook theme class; it is deterministic.
        if (DARK_CLASSES.some((c) => html.classList.contains(c))) return true;
        if (LIGHT_CLASSES.some((c) => html.classList.contains(c))) return false;
        // Otherwise read the painted background (mdBook paints <html>) …
        const rgb = rgbOf(html) || rgbOf(document.body);
        if (rgb) {
            const [r, g, b] = rgb;
            return 0.299 * r + 0.587 * g + 0.114 * b < 128;
        }
        // … and finally the OS preference.
        return prefersDark();
    };

    const initializeWithTheme = () =>
        mermaid.initialize({ startOnLoad: false, theme: isDark() ? 'dark' : 'default' });

    const start = () => {
        initializeWithTheme();
        mermaid.run({ querySelector: '.mermaid' });

        const reloadIfFlipped = () => {
            if (isDark() !== wasDark) window.location.reload();
        };
        // mdBook swaps the theme class on <html>.
        new MutationObserver(reloadIfFlipped).observe(html, {
            attributes: true,
            attributeFilter: ['class', 'style'],
        });
        // …and the OS-following theme changes with the media query.
        if (window.matchMedia) {
            const mq = window.matchMedia('(prefers-color-scheme: dark)');
            if (mq.addEventListener) mq.addEventListener('change', reloadIfFlipped);
            else if (mq.addListener) mq.addListener(reloadIfFlipped);
        }
    };

    const wasDark = isDark();
    // Disable Mermaid's own auto-render before its DOMContentLoaded handler runs;
    // book.js has already applied the theme class by the time this script runs.
    initializeWithTheme();
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', start);
    } else {
        start();
    }
})();