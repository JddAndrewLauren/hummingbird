import { Button } from "../components/core/Button";
import { Icon } from "../components/core/Icon";

// The "a new version is waiting" strip: full width, under the header, above
// the one scroll container. Presentational only — `useAppUpdate` decides
// whether it renders at all.
//
// A prompt rather than a silent auto-reload: an unannounced reload can yank
// the page out from under someone mid-capture. Persistent, with no dismiss —
// it stays until you reload, since the shell it is describing is genuinely
// one deploy behind until then.
//
// The second sentence is load-bearing, not padding. Applying the update is
// an ORIGIN-WIDE act: the button sends `skipWaiting`, and the spec's
// Activate algorithm then hands every client the old worker controlled to
// the new one and fires `controllerchange` in each — which the plugin's own
// registration turns into a `location.reload()` in every tab. There is no
// tab-local version of this gesture (a plain reload never releases control,
// so the worker would stay waiting and the shell stay one deploy behind),
// and convergence is the safe outcome anyway: two builds live at once means
// two content-hashed `core.worker` SharedWorkers over ONE unversioned
// IndexedDB queue (`client/core/src/lib.rs`'s `hummingbird-task::queue`),
// which is ADR-0010's one-core-per-origin invariant broken. Every open tab
// is already showing this strip, so the reader has been told a new version
// exists; what they could not otherwise know is that the click reaches all
// of them. Saying so is what keeps the reload announced rather than merely
// consented to in one window.

export interface UpdateBannerProps {
  onReload: () => void;
}

export function UpdateBanner({ onReload }: UpdateBannerProps) {
  return (
    <div
      // Announced politely, but deliberately NOT focused: `Header.tsx`
      // already moves focus to the `<h1>` on a title change, and two things
      // competing for focus is worse than none.
      role="status"
      aria-live="polite"
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-4)",
        flex: "0 0 auto",
        padding: "var(--space-4) var(--gutter-page)",
        background: "var(--status-info-bg)",
        color: "var(--status-info-fg)",
        borderBottom: "1px solid var(--border-subtle)",
      }}
    >
      <Icon name="sparkles" size={17} />
      <span style={{ font: "var(--type-body-sm)", flex: 1, minWidth: 0 }}>
        A new version of hummingbird is ready. Reloading updates every open tab.
      </span>
      <Button size="sm" variant="quiet" iconLeft="refresh-cw" onClick={onReload}>
        Reload
      </Button>
    </div>
  );
}
