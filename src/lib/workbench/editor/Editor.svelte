<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';

  import './editor.css';
  import type { Locale } from '$lib/paraglide/runtime.js';
  import type { SourceLocation } from '$lib/protocol/generated/SourceLocation';
  import type { Target } from '$lib/protocol/generated/Target';
  import { autocompletion, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
  import {
    defaultKeymap,
    history,
    historyKeymap,
    undo,
    redo,
    undoDepth,
    redoDepth,
    toggleComment,
  } from '@codemirror/commands';
  import {
    bracketMatching,
    indentUnit,
    foldGutter,
    foldKeymap,
    foldAll,
    unfoldAll,
  } from '@codemirror/language';
  import { lintGutter, nextDiagnostic, setDiagnostics, type Diagnostic } from '@codemirror/lint';
  import {
    searchKeymap,
    gotoLine,
    openSearchPanel,
    highlightSelectionMatches,
  } from '@codemirror/search';
  import { Annotation, Compartment, EditorState } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLine,
    highlightActiveLineGutter,
    drawSelection,
    highlightSpecialChars,
    rectangularSelection,
    crosshairCursor,
    dropCursor,
    placeholder,
    panels,
  } from '@codemirror/view';
  import type { Command } from '@codemirror/view';
  import {
    Search,
    Circle,
    ArrowRight,
    ListEnd,
    ListStart,
    Ellipsis,
    Undo2,
    Redo2,
    MessageSquareCode,
    FoldVertical,
    UnfoldVertical,
    CornerDownRight,
  } from '@lucide/svelte';
  import { DropdownMenu } from 'bits-ui';
  import { untrack } from 'svelte';

  import { debugGutter, executionLine } from './debug';
  import { goToLine, retainLineDialog } from './goto-line';
  import { assembly, assemblyHighlighting } from './language';
  import { searchPanel } from './search-panel.svelte';
  import { sourceIndex } from './source';
  import { jumpToLabel } from './structure';

  const {
    value,
    locale,
    target,
    wrap,
    diagnostic,
    oncursor,
    onchange,
    onassemble,
    locations = [],
    breakpoints = [],
    currentLine,
    editable = false,
    onbreakpoint,
    onrun,
    onreveal,
  }: {
    value: string;
    locale: Locale;
    target: Target;
    wrap: boolean;
    diagnostic: Diagnostic | null;
    oncursor: (line: number, column: number) => void;
    onchange: (source: string) => void;
    onassemble?: () => void;
    locations?: readonly SourceLocation[];
    breakpoints?: readonly string[];
    currentLine?: number | undefined;
    editable?: boolean;
    onbreakpoint?: (line: number) => void;
    onrun?: (line: number) => void;
    onreveal?: (line: number) => void;
  } = $props();

  const mapped = $derived(sourceIndex(locations));
  let currentView: EditorView | undefined;
  let editorState = $state.raw<EditorState>();
  const options = $derived({ locale });
  const cursorLine = $derived(editorState?.doc.lineAt(editorState.selection.main.head).number ?? 1);
  const cursorAddresses = $derived([...(mapped.lines.get(cursorLine) ?? [])]);
  const activeBreakpoints = $derived(
    cursorAddresses.filter((address) => breakpoints.includes(address)).length,
  );
  let pendingCommand: Command | undefined;
  const commands = $derived([
    {
      label: m.undo({}, options),
      icon: Undo2,
      run: undo,
      disabled: editorState === undefined || undoDepth(editorState) === 0,
    },
    {
      label: m.redo({}, options),
      icon: Redo2,
      run: redo,
      disabled: editorState === undefined || redoDepth(editorState) === 0,
    },
    { label: m.toggle_comment({}, options), icon: MessageSquareCode, run: toggleComment },
    { label: m.jump_label({}, options), icon: CornerDownRight, run: jumpToLabel, shortcut: 'F12' },
    { label: m.fold_all({}, options), icon: FoldVertical, run: foldAll },
    { label: m.unfold_all({}, options), icon: UnfoldVertical, run: unfoldAll },
  ]);

  function run(command: Command) {
    if (currentView === undefined) return;
    currentView.focus();
    command(currentView);
  }

  /** Reveal the current build diagnostic without replacing editor state or history. */
  export function revealDiagnostic() {
    if (currentView === undefined) return;
    nextDiagnostic(currentView);
    currentView.dispatch({
      effects: EditorView.scrollIntoView(currentView.state.selection.main.head),
    });
    currentView.focus();
  }

  /** Navigate without altering text, undo history or the current search. */
  export function revealLine(line: number) {
    if (currentView === undefined || line < 1 || line > currentView.state.doc.lines) return;
    const position = currentView.state.doc.line(line).from;
    currentView.dispatch({
      selection: { anchor: position },
      effects: EditorView.scrollIntoView(position, { y: 'center' }),
    });
    currentView.focus();
  }

  function translations() {
    const options = { locale };
    return EditorState.phrases.of({
      Completions: m.completions({}, options),
      'Fold line': m.fold_line({}, options),
      'Unfold line': m.unfold_line({}, options),
      'folded code': m.folded_code({}, options),
      'Folded lines': m.folded_lines({}, options),
      'Unfolded lines': m.unfolded_lines({}, options),
      to: m.range_to({}, options),
      'Selection deleted': m.selection_deleted({}, options),
      Find: m.find({}, options),
      'Go to line': m.go_to_line({}, options),
      go: m.go({}, options),
      'replaced match on line $': m.replaced_line({ line: '$' }, options),
      'replaced $ matches': m.replaced_matches({ count: '$' }, options),
      'current match': m.current_match({}, options),
      'on line': m.on_line({}, options),
      'Control character': m.control_character({}, options),
      replace: m.replace({}, options),
      Replace: m.replace({}, options),
      next: m.next({}, options),
      previous: m.previous({}, options),
      all: m.all({}, options),
      'match case': m.match_case({}, options),
      regexp: m.regexp({}, options),
      'by word': m.whole_word({}, options),
      'replace all': m.replace_all({}, options),
      close: m.close({}, options),
    });
  }

  // The attachment owns the view; compartments update settings without replacing history.
  function attachEditor(element: HTMLElement) {
    const panelOverlay = element.appendChild(document.createElement('div'));
    panelOverlay.className = 'editor-panels';
    // CodeMirror themes its container with a required relative flex layout.
    const panelHost = panelOverlay.appendChild(document.createElement('div'));
    panelHost.className = 'editor-panel-host';
    const externalSource = Annotation.define<boolean>();
    const language = new Compartment();
    const syntax = new Compartment();
    const wrapping = new Compartment();
    const accessibility = new Compartment();
    const debugging = new Compartment();
    const execution = new Compartment();
    const view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: untrack(() => value),
        extensions: [
          debugging.of([]),
          lineNumbers(),
          execution.of([]),
          lintGutter(),
          foldGutter(),
          history(),
          drawSelection(),
          highlightSpecialChars(),
          rectangularSelection(),
          crosshairCursor(),
          dropCursor(),
          EditorState.allowMultipleSelections.of(true),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          keymap.of([
            {
              key: 'Mod-Enter',
              run: () => {
                if (onassemble === undefined) return false;
                onassemble();
                return true;
              },
            },
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap.map((binding) =>
              binding.run === gotoLine ? { ...binding, run: goToLine } : binding,
            ),
            ...foldKeymap,
            { key: 'F12', run: jumpToLabel, preventDefault: true },
            { key: 'F8', run: nextDiagnostic },
            {
              key: 'Mod-F10',
              run: (view) => {
                const line = view.state.doc.lineAt(view.state.selection.main.head).number;
                if (!editable || !mapped.lines.has(line) || onrun === undefined) return false;
                onrun(line);
                return true;
              },
            },
            {
              key: 'F9',
              run: (view) => {
                const line = view.state.doc.lineAt(view.state.selection.main.head).number;
                if (!editable || !mapped.lines.has(line) || onbreakpoint === undefined)
                  return false;
                onbreakpoint(line);
                return true;
              },
            },
          ]),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          highlightSelectionMatches(),
          panels({ topContainer: panelHost, bottomContainer: panelHost }),
          searchPanel(() => locale),
          indentUnit.of('    '),
          EditorState.tabSize.of(4),
          assemblyHighlighting,
          syntax.of([]),
          wrapping.of([]),
          language.of([]),
          accessibility.of([]),
          EditorView.updateListener.of((update) => {
            editorState = update.state;
            if (
              update.transactions.some(
                (transaction) => transaction.docChanged && !transaction.annotation(externalSource),
              )
            )
              onchange(update.state.doc.toString());
            if (update.docChanged || update.selectionSet) {
              const position = update.state.selection.main.head;
              const line = update.state.doc.lineAt(position);
              oncursor(line.number, position - line.from + 1);
            }
          }),
          EditorView.darkTheme.of(true),
        ],
      }),
    });
    currentView = view;
    $effect(() => {
      const options = { locale };
      view.dispatch({
        effects: debugging.reconfigure(
          debugGutter(
            mapped.lines,
            breakpoints,
            editable,
            (enabled) =>
              enabled
                ? m.remove_source_breakpoint({}, options)
                : m.add_source_breakpoint({}, options),
            (line) => onbreakpoint?.(line),
          ),
        ),
      });
    });
    $effect(() => {
      view.dispatch({ effects: execution.reconfigure(executionLine(view, currentLine)) });
    });
    $effect(() => {
      view.dispatch({ effects: syntax.reconfigure(assembly(target)) });
    });
    $effect(() => {
      view.dispatch({ effects: wrapping.reconfigure(wrap ? EditorView.lineWrapping : []) });
    });
    $effect(() => {
      const next = value;
      if (!view.state.doc.eq(view.state.toText(next))) {
        // Preserve original BOM/newlines in the document owner until the user edits.
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: next },
          annotations: externalSource.of(true),
        });
      }
    });
    $effect(() => {
      view.dispatch(setDiagnostics(view.state, diagnostic === null ? [] : [diagnostic]));
    });
    $effect(() => {
      retainLineDialog(view, () => {
        view.dispatch({
          effects: [
            language.reconfigure([
              translations(),
              placeholder(m.editor_placeholder({}, { locale })),
            ]),
            accessibility.reconfigure(
              EditorView.contentAttributes.of({ 'aria-label': m.editor_label({}, { locale }) }),
            ),
          ],
        });
      });
    });
    return () => {
      currentView = undefined;
      editorState = undefined;
      view.destroy();
      panelOverlay.remove();
    };
  }
</script>

<div class="editor-toolbar">
  <span>{m.assembly_syntax({}, options)}</span>
  <div>
    {#if onbreakpoint !== undefined && cursorAddresses.length > 0}
      <button
        class="icon-button source-toggle"
        disabled={!editable}
        aria-label={m.source_breakpoint({ line: cursorLine }, options)}
        title={`${m.source_breakpoint({ line: cursorLine }, options)} · F9`}
        aria-pressed={activeBreakpoints === 0
          ? false
          : activeBreakpoints === cursorAddresses.length
            ? true
            : 'mixed'}
        onclick={() => {
          onbreakpoint(cursorLine);
        }}><Circle size={14} aria-hidden="true" /></button
      >
    {/if}
    {#if onrun !== undefined}
      <button
        class="icon-button"
        disabled={!editable || !mapped.lines.has(cursorLine)}
        aria-label={m.run_to_cursor({}, options)}
        title={`${m.run_to_cursor({}, options)} · Ctrl / ⌘ + F10`}
        onclick={() => {
          onrun(cursorLine);
        }}><ListEnd size={14} aria-hidden="true" /></button
      >
    {/if}
    {#if onreveal !== undefined}
      <button
        class="icon-button"
        disabled={editorState === undefined || !mapped.lines.has(cursorLine)}
        aria-label={m.reveal_instruction({}, options)}
        title={m.reveal_instruction({}, options)}
        onclick={() => {
          onreveal(cursorLine);
        }}><CornerDownRight size={15} aria-hidden="true" /></button
      >
    {/if}
    {#if currentLine !== undefined}
      <button
        class="icon-button"
        aria-label={m.reveal_execution({}, options)}
        title={m.reveal_execution({}, options)}
        onclick={() => {
          revealLine(currentLine);
        }}><ArrowRight size={15} aria-hidden="true" /></button
      >
    {/if}
    <button
      class="icon-button"
      aria-label={m.find({}, options)}
      title={m.find({}, options)}
      onclick={() => {
        run(openSearchPanel);
      }}><Search size={15} aria-hidden="true" /></button
    >
    <button
      class="icon-button"
      aria-label={m.go_to_line({}, options)}
      title={m.go_to_line({}, options)}
      onclick={() => {
        run(goToLine);
      }}><ListStart size={15} aria-hidden="true" /></button
    >
    <DropdownMenu.Root>
      <DropdownMenu.Trigger
        class="icon-button"
        aria-label={m.editor_actions({}, options)}
        title={m.editor_actions({}, options)}
        ><Ellipsis size={16} aria-hidden="true" /></DropdownMenu.Trigger
      >
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          class="command-menu"
          align="end"
          sideOffset={6}
          collisionPadding={12}
          onCloseAutoFocus={(event) => {
            if (pendingCommand !== undefined) {
              event.preventDefault();
              run(pendingCommand);
              pendingCommand = undefined;
            }
          }}
        >
          {#each commands as command, index (command.run)}
            {#if index === 2 || index === 4}<DropdownMenu.Separator />{/if}
            <DropdownMenu.Item
              disabled={command.disabled ?? false}
              onSelect={() => {
                pendingCommand = command.run;
              }}
            >
              <command.icon size={15} aria-hidden="true" />{command.label}
              {#if command.shortcut}<kbd>{command.shortcut}</kbd>{/if}
            </DropdownMenu.Item>
          {/each}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  </div>
</div>
<div class="editor" {@attach attachEditor}></div>
