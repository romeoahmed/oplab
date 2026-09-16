/**
 * Map a source byte offset to a CodeMirror position and line/column.
 *
 * @remarks
 * Input offsets count UTF-8 bytes; positions count UTF-16 code units from zero.
 * Lines and columns are one-based, with columns also counting UTF-16 code units.
 * CRLF and CR use CodeMirror's default newline normalization. The boundary inside
 * CRLF belongs to the end of the preceding line.
 *
 * @returns `null` for missing offsets, invalid UTF-8 boundaries or ill-formed Unicode.
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

/** Report line count and original separator style; text without line breaks defaults to LF. */
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
