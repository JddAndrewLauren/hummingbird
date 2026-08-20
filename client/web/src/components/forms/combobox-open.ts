// "A Combobox popup is open" — one fact, held outside React, in
// `shell/app-update.ts`'s listener-set idiom and read the same way, through
// `useSyncExternalStore`.
//
// It exists for Escape and nothing else. `escape-claimants.ts` is the shell's
// single owner of that key: no component binds its own listener, and the one
// place that decides needs every claimant's open flag as shell state. An open
// listbox is the first claimant that lives inside a leaf form control rather
// than in `App.tsx`'s own state, and it sits *inside* the capture popover —
// so without this, the Escape meant for the list would close the whole
// popover and lose the draft behind it.
//
// Threading a callback pair down through `CapturePopover` → `CaptureBox` →
// `Combobox` (and separately through `ItemPanel`) would put the same two
// props on every intermediate component that has no interest in them, for a
// fact only the shell's keydown reads. A module-level signal is the cheaper
// shape and the one the shell already has an idiom for.
//
// The snapshot is a **boolean primitive** on purpose: `useSyncExternalStore`
// compares by reference, and a `getSnapshot` minting a fresh object per call
// re-renders forever (`app-update.ts` froze one object for the same reason).

type Listener = () => void;
type Close = () => void;

export interface ComboboxOpenSignal {
  subscribe(listener: Listener): () => void;
  /** Whether any combobox popup is open. */
  getSnapshot(): boolean;
  /** Declares this popup open and hands over the way to shut it; the
   * returned function withdraws it. A component registers in an effect keyed
   * on its own open state, so the withdrawal covers both closing and
   * unmounting-while-open. */
  register(close: Close): () => void;
  /** Shuts every open popup — the shell's Escape closer. Plural because the
   * registry cannot prove there is only one; in practice a second list
   * cannot be opened without the first blurring shut. */
  closeAll(): void;
}

export function createComboboxOpenSignal(): ComboboxOpenSignal {
  const open = new Set<Close>();
  const listeners = new Set<Listener>();

  function announce(): void {
    for (const listener of listeners) {
      listener();
    }
  }

  return {
    subscribe(listener: Listener): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getSnapshot(): boolean {
      return open.size > 0;
    },
    register(close: Close): () => void {
      open.add(close);
      announce();
      return () => {
        // Only announce on a real removal: a deregistration that changes
        // nothing (React re-running an effect cleanup after the set was
        // already drained by `closeAll`) is not a state change.
        if (open.delete(close)) {
          announce();
        }
      };
    },
    closeAll(): void {
      if (open.size === 0) {
        return;
      }
      // Drain first. Each `close` drives a React state update whose effect
      // cleanup deregisters, and iterating the live set while it mutates
      // would skip entries.
      const closers = [...open];
      open.clear();
      for (const close of closers) {
        close();
      }
      announce();
    },
  };
}

/** The one signal the shell reads, written by every mounted `Combobox`. */
export const comboboxOpenSignal = createComboboxOpenSignal();
