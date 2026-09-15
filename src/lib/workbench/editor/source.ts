/** Map an original UTF-8 byte boundary to CodeMirror's UTF-16 position and 1-based line/column.
 *
 * CRLF and CR use CodeMirror's default newline normalization. The boundary inside
 * CRLF belongs to the end of the preceding line. Invalid boundaries have no location.
 */
export function sourceLocation(source: string, offset: number | null) {
  if (offset === null || !Number.isSafeInteger(offset) || offset < 0 || !source.isWellFormed())
    return null;
  const bytes = new TextEncoder().encode(source);
  if (offset > bytes.length) return null;
  try {
    let prefix = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(
      bytes.subarray(0, offset),
    );
    if (prefix.endsWith('\r') && bytes[offset] === 10) prefix = prefix.slice(0, -1);
    const normalized = prefix.replace(/\r\n?/g, '\n');
    return {
      position: normalized.length,
      line: normalized.split('\n').length,
      column: normalized.length - normalized.lastIndexOf('\n'),
    };
  } catch {
    return null;
  }
}

/** Count logical lines and report the original separators without normalizing the source. */
export function sourceInfo(source: string) {
  const endings = new Set<string>();
  let lines = 1;
  for (const [ending] of source.matchAll(/\r\n?|\n/g)) {
    endings.add(ending);
    lines += 1;
  }
  return {
    lines,
    ending:
      endings.size > 1 ? 'mixed' : endings.has('\r\n') ? 'CRLF' : endings.has('\r') ? 'CR' : 'LF',
  };
}
