/**
 * Escapes HTML special characters to prevent Cross-Site Scripting (XSS).
 *
 * Used wherever user input or backend data is inserted into HTML
 * (e.g. in dialogs, tables, tooltips). Also replaces line breaks
 * (both literal and escaped) with spaces so they don't interfere
 * in single-line contexts.
 *
 * @param {string} str - The string to escape.
 * @returns {string} The escaped string, safe for HTML output.
 */
export function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')       // & → &amp; (must be first)
    .replace(/</g, '&lt;')        // < → &lt;
    .replace(/>/g, '&gt;')        // > → &gt;
    .replace(/"/g, '&quot;')      // " → &quot;
    .replace(/'/g, '&#039;')      // ' → &#039;
    .replace(/\\n/g, ' ')         // Escaped line breaks (\n as literal) → spaces
    .replace(/\n/g, ' ');          // Actual line breaks → spaces
}
