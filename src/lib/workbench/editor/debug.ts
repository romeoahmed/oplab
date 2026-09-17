import { Decoration, EditorView, GutterMarker, gutter } from '@codemirror/view';

class BreakpointMarker extends GutterMarker {
  constructor(
    readonly pressed: boolean | 'mixed',
    readonly editable: boolean,
    readonly title: string,
  ) {
    super();
  }
  eq(other: BreakpointMarker) {
    return (
      this.pressed === other.pressed &&
      this.editable === other.editable &&
      this.title === other.title
    );
  }
  toDOM(view: EditorView) {
    const marker = view.dom.ownerDocument.createElement('span');
    marker.className = 'source-breakpoint';
    marker.title = this.title;
    marker.dataset.state = String(this.pressed);
    marker.dataset.editable = String(this.editable);
    return marker;
  }
}

/** Display the owner's current source mappings; the owner clears them on edits. */
export function debugGutter(
  lines: ReadonlyMap<number, ReadonlySet<string>>,
  breakpoints: readonly string[],
  editable: boolean,
  label: (enabled: boolean) => string,
  toggle: (line: number) => void,
) {
  const active = new Set(breakpoints);
  return gutter({
    class: 'cm-source-gutter',
    domEventHandlers: {
      mousedown: (view, block, event) => {
        if (!(event instanceof MouseEvent) || event.button !== 0) return false;
        const line = view.state.doc.lineAt(block.from).number;
        if (!editable || !lines.has(line)) return false;
        toggle(line);
        return true;
      },
    },
    lineMarker: (view, block) => {
      const line = view.state.doc.lineAt(block.from).number;
      const addresses = lines.get(line);
      if (addresses === undefined) return null;
      const enabled = [...addresses].filter((address) => active.has(address)).length;
      const pressed = enabled === 0 ? false : enabled === addresses.size ? true : 'mixed';
      return new BreakpointMarker(pressed, editable, editable ? label(pressed === true) : '');
    },
  });
}

export function executionLine(view: EditorView, line: number | undefined) {
  return EditorView.decorations.of(
    line === undefined || line > view.state.doc.lines
      ? Decoration.none
      : Decoration.set([
          Decoration.line({ class: 'cm-execution-line' }).range(view.state.doc.line(line).from),
        ]),
  );
}
