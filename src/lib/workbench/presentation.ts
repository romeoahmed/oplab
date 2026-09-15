import type { Status } from '$lib/protocol/generated/Status';
import type { Locale } from '$lib/paraglide/runtime';
import * as m from '$lib/paraglide/messages.js';

export function stateLabel(status: Status | undefined, locale: Locale): string {
  const options = { locale };
  if (status === undefined) return m.no_session({}, options);
  switch (status.type) {
    case 'ready':
      return m.state_ready({}, options);
    case 'running':
      return m.state_running({}, options);
    case 'paused':
      return m.state_paused({}, options);
    case 'stepped':
      return m.state_stepped({}, options);
    case 'breakpoint':
      return m.state_breakpoint({}, options);
    case 'crashed':
      return m.state_crashed({}, options);
    case 'terminated':
      switch (status.data) {
        case 'completed':
          return m.state_completed({}, options);
        case 'cancelled':
          return m.state_cancelled({}, options);
        case 'budget':
          return m.state_budget({}, options);
        case 'guest_fault':
          return m.state_fault({}, options);
        case 'unsupported_environment':
          return m.state_environment({}, options);
      }
  }
}

export function problemLabel(code: string, locale: Locale): string {
  const options = { locale };
  switch (code) {
    case 'assembly':
      return m.error_assembly({}, options);
    case 'completion':
      return m.error_completion({}, options);
    case 'deadline':
      return m.error_deadline({}, options);
    case 'memory_limit':
      return m.error_memory({}, options);
    case 'storage':
      return m.error_storage({}, options);
    case 'invalid_state':
    case 'stale_session':
      return m.error_state({}, options);
    case 'input':
    case 'invalid_input':
    case 'resource_limit':
      return m.error_input({}, options);
    default:
      return m.error_engine({}, options);
  }
}
