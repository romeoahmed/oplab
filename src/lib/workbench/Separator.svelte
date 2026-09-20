<script lang="ts">
  import './separator.css';

  const {
    axis,
    label,
    controls,
    value,
    minimum,
    maximum,
    defaultValue,
    onchange,
  }: {
    axis: 'width' | 'height';
    label: string;
    controls: string;
    value: number;
    minimum: number;
    maximum: number;
    defaultValue: number;
    onchange: (value: number) => void;
  } = $props();
  let drag: { pointer: number; position: number; value: number; size: number } | undefined;
  const update = (next: number) => {
    onchange(Math.round(Math.max(minimum, Math.min(maximum, next))));
  };

  function start(event: PointerEvent) {
    if (event.button !== 0 || drag !== undefined) return;
    const element = event.currentTarget;
    if (!(element instanceof HTMLElement) || element.parentElement === null) return;
    const area = element.parentElement.getBoundingClientRect();
    const pane = document.getElementById(controls)?.getBoundingClientRect();
    const size = axis === 'width' ? area.width : window.innerHeight;
    if (size <= 0 || pane === undefined) return;
    drag = {
      pointer: event.pointerId,
      position: axis === 'width' ? event.clientX : event.clientY,
      value: (100 * (axis === 'width' ? pane.width : pane.height)) / size,
      size,
    };
    element.setPointerCapture(event.pointerId);
    element.focus();
    event.preventDefault();
  }
  function move(event: PointerEvent) {
    if (drag?.pointer !== event.pointerId) return;
    const position = axis === 'width' ? event.clientX : event.clientY;
    update(drag.value + (100 * (drag.position - position)) / drag.size);
  }
  function keyboard(event: KeyboardEvent) {
    const increase = axis === 'width' ? 'ArrowLeft' : 'ArrowUp';
    const decrease = axis === 'width' ? 'ArrowRight' : 'ArrowDown';
    const step = event.shiftKey ? 5 : 1;
    const next =
      event.key === increase
        ? value + step
        : event.key === decrease
          ? value - step
          : event.key === 'Home'
            ? minimum
            : event.key === 'End'
              ? maximum
              : undefined;
    if (next === undefined) return;
    event.preventDefault();
    update(next);
  }
</script>

<!-- WAI-ARIA's adjustable window splitter is a focusable separator; Svelte classifies only its static variant. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="pane-separator"
  class:column-separator={axis === 'width'}
  class:row-separator={axis === 'height'}
  role="separator"
  tabindex="0"
  aria-label={label}
  aria-controls={controls}
  aria-orientation={axis === 'width' ? 'vertical' : 'horizontal'}
  aria-valuemin={minimum}
  aria-valuemax={maximum}
  aria-valuenow={value}
  aria-valuetext={`${String(value)}%`}
  onpointerdown={start}
  onpointermove={move}
  onlostpointercapture={(event) => {
    if (drag?.pointer === event.pointerId) drag = undefined;
  }}
  onpointerup={(event) => {
    if (drag?.pointer !== event.pointerId) return;
    const element = event.currentTarget;
    if (element.hasPointerCapture(event.pointerId)) element.releasePointerCapture(event.pointerId);
    drag = undefined;
  }}
  onkeydown={keyboard}
  ondblclick={() => {
    update(defaultValue);
  }}
></div>
