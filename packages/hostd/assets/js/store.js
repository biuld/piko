// Minimal state store with per-action notifications.

export function createStore(initial) {
  let state = { ...initial };
  const handlers = [];
  return {
    get state() {
      return state;
    },
    subscribe(handler) {
      handlers.push(handler);
      return () => {
        const i = handlers.indexOf(handler);
        if (i >= 0) handlers.splice(i, 1);
      };
    },
    set(patch, action) {
      state = { ...state, ...patch };
      for (const handler of [...handlers]) handler(state, action);
    },
  };
}
