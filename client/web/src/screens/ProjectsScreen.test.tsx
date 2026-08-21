// @vitest-environment jsdom
//
// The gate for #624's screen. `client/web` has a documented trap — it can ship
// UI state with no reader, and `tsc` cannot see a missing caller — so what
// these assert is that the screen renders **from `TaskState`**, not that its
// pieces exist.
import { describe, expect, it, vi } from "vitest";
import {
  blockedEntryDTO,
  fireEvent,
  itemDTO,
  ledgerRowDTO,
  projectDTO,
  projectLinkDTO,
  render,
  routeDTO,
  screen,
  taskState,
  within,
} from "../test/component";
import { ProjectsScreen, type ProjectsScreenProps } from "./ProjectsScreen";

const noop = () => {};
/** The board's clock. Fixed, so nothing on this screen reads a wall clock. */
const NOW_MS = Date.parse("2026-03-04T10:00:00Z");
/** `onPatchProject` returns its minted seed (batch review, projects-dossier
 * #668) — `ArchiveCard` needs it to recognise its own write. Tests that do
 * not care what the seed is (most of this file) pass this default rather
 * than `noop`, which cannot satisfy the `=> string` return type. */
const noopPatchProject = (): string => "unused-seed";
/** Same shape as `noopPatchProject`, for the four other writes #669 gave a
 * minted-seed return to (`useProjectsWiring.ts`'s own doc) — a test that
 * does not care what the seed is passes this rather than `noop`. */
const noopWriteSeed = (): string => "unused-seed";

/** A fresh in-memory `storage` per render, for the board's own view
 * preferences. Not optional: the screen falls back to the ambient
 * `localStorage`, so an axis switched in one test would be read by every
 * later one — `NowScreen.test.tsx`'s `memoryStorage` carries the full
 * argument, including why this repo's local vitest has no `localStorage` at
 * all while CI does. */
function memoryStorage(seed: Record<string, string> = {}) {
  const entries = { ...seed };
  return {
    entries,
    getItem: (key: string) => entries[key] ?? null,
    setItem: (key: string, value: string) => {
      entries[key] = value;
    },
    removeItem: (key: string) => {
      delete entries[key];
    },
  };
}

/** #626 added three more required props (the links card's read/write doors)
 * — every call site here cares only about the
 * project create/patch behaviour this file already covers, so this helper
 * supplies the new ones' defaults once rather than repeating them at every
 * `render` call. The refetch key rides `task.syncOutcomeSeq`, not a prop. */
function renderProjectsScreen(props: Partial<ProjectsScreenProps> & Pick<ProjectsScreenProps, "task">) {
  const element = (next: Partial<ProjectsScreenProps> & Pick<ProjectsScreenProps, "task">) => (
    <ProjectsScreen
      onCreateProject={noop}
      onPatchProject={noopPatchProject}
      onRequestProjectLinks={noop}
      onCreateProjectLink={noopWriteSeed}
      onPatchProjectLink={noopWriteSeed}
      onRequestRoute={noop}
      onPatchRoute={noopWriteSeed}
      nowMs={NOW_MS}
      selectedItemId={null}
      onOpenItem={noop}
      onCloseItemDetail={noop}
      onAct={noop}
      storage={memoryStorage()}
      {...next}
    />
  );
  const result = render(element(props));
  return {
    ...result,
    /** The same render with new props — a completed cycle landing on an open
     * dossier, which is when the properties card decides what to re-seed. */
    rerenderWith: (next: Partial<ProjectsScreenProps> & Pick<ProjectsScreenProps, "task">) =>
      result.rerender(element(next)),
  };
}

describe("ProjectsScreen", () => {
  it("holds rather than claiming 'no projects' while the read has not answered", () => {
    renderProjectsScreen({ task: taskState(), onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByText("Reading projects…")).toBeTruthy();
    expect(screen.queryByText("No projects yet")).toBeNull();
  });

  it("renders the empty state only for a real, empty answer", () => {
    renderProjectsScreen({ task: taskState({ projects: [] }), onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByText("No projects yet")).toBeTruthy();
  });

  it("builds the grid from TaskState, with counts derived from the ledger", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [
        ledgerRowDTO({ id: "i-1", projectId: "p-1", stage: "ready" }),
        ledgerRowDTO({ id: "i-2", projectId: "p-1", stage: "done" }),
      ],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByRole("heading", { level: 3, name: "House repairs" })).toBeTruthy();
    expect(screen.getByText("1 action · 1 done")).toBeTruthy();
    expect(screen.getByText("1 live · 0 archived")).toBeTruthy();
  });

  // The archived half arrives on `archivedProjects`, NOT on `projects` — an
  // archived project is absent in the mirror, so the live read cannot carry
  // one (`Core::archived_projects`). A fixture that seeded it into `projects`
  // is exactly how this toggle first shipped working everywhere but the app.
  it("hides archived projects until the toggle, and says how many are hidden", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      archivedProjects: [projectDTO({ id: "p-9", name: "Old bike", archivedAt: 5_000 })],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByText("1 live · 1 archived")).toBeTruthy();
    expect(screen.queryByRole("heading", { level: 3, name: "Old bike" })).toBeNull();

    fireEvent.click(screen.getByRole("switch", { name: "Show archived" }));

    expect(screen.getByRole("heading", { level: 3, name: "Old bike" })).toBeTruthy();
    expect(screen.getByText("archived")).toBeTruthy();
  });

  it("sends the trimmed name to onCreateProject and refuses a blank one", () => {
    const onCreateProject = vi.fn();
    renderProjectsScreen({ task: taskState({ projects: [], ledger: [] }), onCreateProject: onCreateProject, onPatchProject: noopPatchProject });

    const create = screen.getByRole("button", { name: "Create" });
    expect(create.hasAttribute("disabled")).toBe(true);

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "  Rebuild the deck  " } });
    fireEvent.click(create);

    expect(onCreateProject).toHaveBeenCalledWith("Rebuild the deck");
  });

  // The no-overlay contract's whole visible consequence: between the enqueue
  // and the cycle that pulls it back there is nothing in `projects` to show,
  // so the screen has to say it is waiting rather than look like it dropped
  // the input.
  it("says it is waiting while a created project has not yet come back", () => {
    const task = taskState({
      projects: [],
      ledger: [],
      lastProjectWrite: { seed: "s-1", projectId: "p-new", kind: "ok", error: null },
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByText("creating — appears when the round trip lands")).toBeTruthy();
  });

  it("drops the waiting line once the project reaches the grid", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-new", name: "Rebuild the deck" })],
      ledger: [],
      lastProjectWrite: { seed: "s-1", projectId: "p-new", kind: "ok", error: null },
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.queryByText("creating — appears when the round trip lands")).toBeNull();
    expect(screen.getByRole("heading", { level: 3, name: "Rebuild the deck" })).toBeTruthy();
  });

  it("renders a failed write's own message rather than swallowing it", () => {
    const task = taskState({
      projects: [],
      ledger: [],
      lastProjectWrite: {
        seed: "s-1",
        projectId: null,
        kind: "failed",
        error: "name must be non-empty",
      },
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByText("name must be non-empty")).toBeTruthy();
  });

  // #624's "busy" state. The worker DROPS a request it cannot serve rather
  // than queueing it (`store.ts`), so this is a create that never happened,
  // and it arrives with no `error` of its own — the one shape where silence
  // would look exactly like success.
  it("states a busy drop rather than rendering nothing", () => {
    const task = taskState({
      projects: [],
      ledger: [],
      lastProjectWrite: { seed: "s-1", projectId: null, kind: "busy", error: null },
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    expect(screen.getByText("That project write did not go through.")).toBeTruthy();
    expect(screen.queryByText("creating — appears when the round trip lands")).toBeNull();
  });

  it("opens a project to the dossier shell and comes back to the grid", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.getByRole("heading", { level: 2, name: "House repairs" })).toBeTruthy();
    expect(screen.getByText("route · destination")).toBeTruthy();
    // The centre column is the board, so an empty project says so in the
    // board's own words rather than Now's global "Nothing to start".
    expect(screen.getByText("Nothing startable in this project")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "All projects" }));

    expect(screen.getByRole("heading", { level: 3, name: "House repairs" })).toBeTruthy();
  });

  // #625: the properties card.
  it("renders the stored github repo as a derived link, not the stored value itself", () => {
    const task = taskState({
      projects: [
        projectDTO({ id: "p-1", name: "House repairs", githubRepo: "JddAndrewLauren/hummingbird" }),
      ],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    const link = screen.getByRole("link", { name: "Open GitHub repo" });
    expect(link.getAttribute("href")).toBe("https://github.com/JddAndrewLauren/hummingbird");
    expect((screen.getByLabelText("GitHub repo") as HTMLInputElement).value).toBe(
      "JddAndrewLauren/hummingbird",
    );
  });

  // The glyph beside the label is the only trace of the URL on this card, so
  // its absence when there is no repo is the whole of the empty case.
  it("renders no repo link when the project has no github repo", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.queryByRole("link", { name: "Open GitHub repo" })).toBeNull();
  });

  it("sends only the changed properties fields to onPatchProject", () => {
    const onPatchProject = vi.fn();
    const project = projectDTO({ id: "p-1", name: "House repairs" });
    const task = taskState({ projects: [project], ledger: [] });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: onPatchProject });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    fireEvent.change(screen.getByLabelText("GitHub repo"), {
      target: { value: "JddAndrewLauren/hummingbird" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onPatchProject).toHaveBeenCalledWith(project, { githubRepo: "JddAndrewLauren/hummingbird" });
  });

  it("clears a field by saving it empty", () => {
    const onPatchProject = vi.fn();
    const project = projectDTO({ id: "p-1", name: "House repairs", defaultContext: "@computer" });
    const task = taskState({ projects: [project], ledger: [] });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: onPatchProject });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    fireEvent.change(screen.getByLabelText("Default context"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onPatchProject).toHaveBeenCalledWith(project, { defaultContext: null });
  });

  it("does not paint another project's failed write into this dossier", () => {
    const task = taskState({
      projects: [
        projectDTO({ id: "p-1", name: "House repairs" }),
        projectDTO({ id: "p-2", name: "Garden" }),
      ],
      ledger: [],
      lastProjectWrite: { seed: "s-1", projectId: "p-2", kind: "failed", error: "no can do" },
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.queryByText("no can do")).toBeNull();
  });

  // #669's sixth reader: `PropertiesCard` used to gate its failure read on
  // `project.id` alone, so a failed `ArchiveCard` write on the SAME
  // dossier — a different seed, sharing this card's one `lastProjectWrite`
  // broadcast slot — satisfied it just as readily as its own. Asserting on
  // the rendered label, not on internal state, is what makes this fail
  // against the pre-fix predicate.
  it("does not paint a sibling card's failed write — same project, a different seed — into this card", () => {
    const project = projectDTO({ id: "p-1", name: "House repairs" });
    const task = taskState({ projects: [project], ledger: [] });
    const { rerenderWith } = renderProjectsScreen({ task, onPatchProject: () => "properties-seed" });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    fireEvent.change(screen.getByLabelText("GitHub repo"), {
      target: { value: "JddAndrewLauren/hummingbird" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // The sibling `ArchiveCard`'s own write fails and broadcasts, with a
    // different seed — this card issued no such write.
    rerenderWith({
      task: {
        ...task,
        lastProjectWrite: { seed: "archive-seed", projectId: "p-1", kind: "failed", error: "no can do" },
      },
      onPatchProject: () => "properties-seed",
    });

    expect(screen.queryByText("no can do")).toBeNull();
  });

  it("disables Save until a field actually changes", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noopPatchProject });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(true);

    fireEvent.change(screen.getByLabelText("GitHub repo"), { target: { value: "a/b" } });

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(false);
  });

  it("keeps a field typed while another field's write was in flight", () => {
    // Both properties share one Save and one version, so the repo's write
    // landing moves the version while a default context is half-typed. The
    // re-seed must skip the field being edited, or the typing is lost.
    const project = projectDTO({ id: "p-1", name: "House repairs" });
    const task = taskState({ projects: [project], ledger: [] });
    const { rerenderWith } = renderProjectsScreen({ task });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    fireEvent.change(screen.getByLabelText("GitHub repo"), { target: { value: "a/b" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    fireEvent.change(screen.getByLabelText("Default context"), { target: { value: "@computer" } });

    const landed = { ...project, githubRepo: "a/b", version: 2 };
    rerenderWith({ task: { ...task, projects: [landed] } });

    expect((screen.getByLabelText("Default context") as HTMLInputElement).value).toBe("@computer");
    expect((screen.getByLabelText("GitHub repo") as HTMLInputElement).value).toBe("a/b");
  });

  it("re-seeds an untouched field from a write another device made", () => {
    const project = projectDTO({ id: "p-1", name: "House repairs" });
    const task = taskState({ projects: [project], ledger: [] });
    const { rerenderWith } = renderProjectsScreen({ task });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    const landed = { ...project, defaultContext: "@errands", version: 2 };
    rerenderWith({ task: { ...task, projects: [landed] } });

    expect((screen.getByLabelText("Default context") as HTMLInputElement).value).toBe("@errands");
  });

  // #630: the archive card — the dossier's last placeholder.
  describe("the archive card", () => {
    it("names the live item count before the human commits to archiving", () => {
      const project = projectDTO({ id: "p-1", name: "House repairs" });
      const task = taskState({
        projects: [project],
        ledger: [
          ledgerRowDTO({ id: "i-1", projectId: "p-1" }),
          ledgerRowDTO({ id: "i-2", projectId: "p-1" }),
          ledgerRowDTO({ id: "i-3", projectId: "p-2" }),
        ],
      });
      renderProjectsScreen({ task });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));

      expect(screen.getByText("Archiving takes 2 live items down with it.")).toBeTruthy();
    });

    it("names a project with no live items honestly, rather than claiming a live count", () => {
      const project = projectDTO({ id: "p-1", name: "House repairs" });
      const task = taskState({ projects: [project], ledger: [] });
      renderProjectsScreen({ task });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));

      expect(
        screen.getByText("This project has no live items — archiving takes down the project alone."),
      ).toBeTruthy();
    });

    it("archives only on confirm, and cancel dismisses with no write", () => {
      const onPatchProject = vi.fn();
      const project = projectDTO({ id: "p-1", name: "House repairs" });
      const task = taskState({ projects: [project], ledger: [] });
      renderProjectsScreen({ task, onPatchProject });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

      expect(onPatchProject).not.toHaveBeenCalled();
      expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();

      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));
      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));

      expect(onPatchProject).toHaveBeenCalledWith(project, { archivedAt: expect.any(Number) });
    });

    it("offers Unarchive on an archived project, with no confirm step", () => {
      const onPatchProject = vi.fn();
      const project = projectDTO({ id: "p-1", name: "House repairs", archivedAt: 9_000 });
      const task = taskState({ projects: [], archivedProjects: [project], ledger: [] });
      renderProjectsScreen({ task, onPatchProject });
      fireEvent.click(screen.getByRole("switch", { name: "Show archived" }));
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      expect(screen.queryByRole("button", { name: "Archive project" })).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Unarchive" }));

      expect(onPatchProject).toHaveBeenCalledWith(project, { archivedAt: null });
    });

    it("clears the pending flag and shows the failure on a dropped write, not a silent stall", () => {
      const project = projectDTO({ id: "p-1", name: "House repairs" });
      const task = taskState({ projects: [project], ledger: [] });
      // Must return the seed the simulated broadcast below carries — since
      // #668's fix, `ArchiveCard` only reacts to a `lastProjectWrite` whose
      // `seed` matches the one its own write minted.
      const { rerenderWith } = renderProjectsScreen({ task, onPatchProject: () => "s-1" });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));
      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));

      rerenderWith({
        task: {
          ...task,
          lastProjectWrite: { seed: "s-1", projectId: "p-1", kind: "failed", error: "no can do" },
        },
        onPatchProject: () => "s-1",
      });

      // `lastProjectWrite` is one broadcast slot shared by every card that
      // writes a project field (`PropertiesCard`'s own doc) — both it and
      // this card read the same failed write, so `getAllByText` rather than
      // `getByText`, same as the properties card's own failure test does
      // not need to because only one card existed when it was written.
      expect(screen.getAllByText("no can do").length).toBeGreaterThan(0);
      expect(screen.getByRole("button", { name: "Archive project" }).hasAttribute("disabled")).toBe(false);
    });

    it("does not paint another project's failed write into this dossier", () => {
      const project = projectDTO({ id: "p-1", name: "House repairs" });
      const task = taskState({
        projects: [project, projectDTO({ id: "p-2", name: "Garden" })],
        ledger: [],
        lastProjectWrite: { seed: "s-1", projectId: "p-2", kind: "failed", error: "no can do" },
      });
      renderProjectsScreen({ task });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      expect(screen.queryByText("no can do")).toBeNull();
    });

    // Batch review, projects-dossier #668 (FINAL-GATE finding 2): a failed
    // *properties* write used to satisfy this card's `failedHere` on
    // `projectId` alone — both cards share the one `lastProjectWrite`
    // broadcast slot for the same project — clearing `pending` while this
    // card's own archive write was still queued. `confirming` was never
    // touched by that path, so the confirm dialog stayed open over a
    // re-enabled "Archive project" button while the real write was still
    // in flight: a double-submit that 409s into the dead-letter journal.
    it("does not let a failed properties write clear this card's pending or re-arm its button", () => {
      const project = projectDTO({ id: "p-1", name: "House repairs" });
      const task = taskState({ projects: [project], ledger: [] });
      // The archive write's own seed — distinct from the properties write's
      // seed simulated below, so a correct fix must tell them apart.
      const { rerenderWith } = renderProjectsScreen({ task, onPatchProject: () => "archive-seed" });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));
      fireEvent.click(screen.getByRole("button", { name: "Archive project" }));
      expect(screen.getByRole("button", { name: "Archiving…" }).hasAttribute("disabled")).toBe(true);

      // A sibling `PropertiesCard` write on the SAME project fails and
      // broadcasts, with a different seed — this card issued no such write.
      rerenderWith({
        task: {
          ...task,
          lastProjectWrite: { seed: "properties-seed", projectId: "p-1", kind: "failed", error: "no can do" },
        },
        onPatchProject: () => "archive-seed",
      });

      // Still pending, still confirming — the archive write this card
      // issued is still queued, so the button must stay disabled and the
      // confirm dialog must not re-arm a second submit.
      expect(screen.getByRole("button", { name: "Archiving…" }).hasAttribute("disabled")).toBe(true);
      expect(screen.queryByRole("button", { name: "Archive project" })).toBeNull();
    });
  });

  // #626: the links card.
  describe("the links card", () => {
    function openDossier(
      overrides: {
        linksByProject?: ReturnType<typeof taskState>["linksByProject"];
        lastProjectLinkWrite?: ReturnType<typeof taskState>["lastProjectLinkWrite"];
        onRequestProjectLinks?: ProjectsScreenProps["onRequestProjectLinks"];
        onCreateProjectLink?: ProjectsScreenProps["onCreateProjectLink"];
        onPatchProjectLink?: ProjectsScreenProps["onPatchProjectLink"];
      } = {},
    ) {
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        linksByProject: overrides.linksByProject ?? {},
        lastProjectLinkWrite: overrides.lastProjectLinkWrite ?? null,
      });
      const { rerenderWith } = renderProjectsScreen({
        task,
        onRequestProjectLinks: overrides.onRequestProjectLinks ?? noop,
        onCreateProjectLink: overrides.onCreateProjectLink ?? noopWriteSeed,
        onPatchProjectLink: overrides.onPatchProjectLink ?? noopWriteSeed,
      });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));
      return { task, rerenderWith };
    }

    it("requests this project's links the moment the dossier opens", () => {
      const onRequestProjectLinks = vi.fn();
      openDossier({ onRequestProjectLinks });

      expect(onRequestProjectLinks).toHaveBeenCalledWith("p-1");
    });

    it("re-requests links on every completed cycle the dossier stays open for", () => {
      const onRequestProjectLinks = vi.fn();
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        syncOutcomeSeq: 1,
      });
      const { rerender } = renderProjectsScreen({ task, onRequestProjectLinks });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));
      expect(onRequestProjectLinks).toHaveBeenCalledTimes(1);

      rerender(
        <ProjectsScreen
          task={{ ...task, syncOutcomeSeq: 2 }}
          onCreateProject={noop}
          onPatchProject={noopPatchProject}
          onRequestProjectLinks={onRequestProjectLinks}
          onCreateProjectLink={noopWriteSeed}
          onPatchProjectLink={noopWriteSeed}
          onRequestRoute={noop}
          onPatchRoute={noopWriteSeed}
          nowMs={NOW_MS}
          selectedItemId={null}
          onOpenItem={noop}
          onCloseItemDetail={noop}
          onAct={noop}
          storage={memoryStorage()}
        />,
      );

      expect(onRequestProjectLinks).toHaveBeenCalledTimes(2);
    });

    it("holds rather than claiming 'no links' while the read has not answered", () => {
      openDossier();

      expect(screen.getByText("Reading links…")).toBeTruthy();
      expect(screen.queryByText("No links yet.")).toBeNull();
    });

    it("renders the empty state only for a real, empty answer", () => {
      openDossier({ linksByProject: { "p-1": [] } });

      expect(screen.getByText("No links yet.")).toBeTruthy();
    });

    it("renders every live link, position-ordered, labelled or bare", () => {
      openDossier({
        linksByProject: {
          "p-1": [
            projectLinkDTO({ id: "l-2", url: "https://b.example", position: 2 }),
            projectLinkDTO({ id: "l-1", url: "https://a.example", label: "A site", position: 1 }),
          ],
        },
      });

      const links = screen.getAllByRole("link").filter((el) => el.getAttribute("href")?.includes("example"));
      expect(links.map((el) => el.textContent)).toEqual(["A site", "https://b.example"]);
    });

    it("sends the trimmed url, label and next position to onCreateProjectLink", () => {
      const onCreateProjectLink = vi.fn();
      openDossier({
        linksByProject: { "p-1": [projectLinkDTO({ id: "l-1", position: 1 })] },
        onCreateProjectLink,
      });

      fireEvent.change(screen.getByLabelText("URL"), {
        target: { value: "  https://docs.example  " },
      });
      fireEvent.change(screen.getByLabelText("Label"), { target: { value: "  Docs  " } });
      fireEvent.click(screen.getByRole("button", { name: "Add link" }));

      expect(onCreateProjectLink).toHaveBeenCalledWith("p-1", "https://docs.example", "Docs", 2);
    });

    it("mints the next position past a gap left by a removed row, never onto a live one", () => {
      // Positions 0,1,2 minus the (soft-removed, so absent here) middle row:
      // live 0 and 2 with length 2. A count-minted position would be 2 —
      // duplicating the live row at 2 and turning its reorder swap into a
      // value-identical no-op. The mint must come off the max instead.
      const onCreateProjectLink = vi.fn();
      openDossier({
        linksByProject: {
          "p-1": [
            projectLinkDTO({ id: "l-1", url: "https://a.example", position: 0 }),
            projectLinkDTO({ id: "l-3", url: "https://c.example", position: 2 }),
          ],
        },
        onCreateProjectLink,
      });

      fireEvent.change(screen.getByLabelText("URL"), { target: { value: "https://d.example" } });
      fireEvent.click(screen.getByRole("button", { name: "Add link" }));

      expect(onCreateProjectLink).toHaveBeenCalledWith("p-1", "https://d.example", null, 3);
    });

    it("refuses to add before the read has answered, so no position is minted blind", () => {
      // "No links yet" and "not known yet" would both mint position 0, and
      // against a project that does have links that duplicates the first
      // row's — the same collision the max-minted position avoids.
      openDossier();

      fireEvent.change(screen.getByLabelText("URL"), { target: { value: "https://d.example" } });

      expect(screen.getByText("Reading links…")).toBeTruthy();
      expect(screen.getByRole("button", { name: "Add link" }).hasAttribute("disabled")).toBe(true);
    });

    it("refuses a blank url", () => {
      openDossier({ linksByProject: { "p-1": [] } });

      expect(screen.getByRole("button", { name: "Add link" }).hasAttribute("disabled")).toBe(true);
    });

    it("edits a link in place and sends only url and label to onPatchProjectLink", () => {
      const onPatchProjectLink = vi.fn();
      const link = projectLinkDTO({ id: "l-1", url: "https://old.example", label: null });
      openDossier({ linksByProject: { "p-1": [link] }, onPatchProjectLink });

      fireEvent.click(screen.getByRole("button", { name: "Edit" }));
      // The edit row renders before the card's own add-link form, so its
      // "URL" input is the first of the two.
      const urlInputs = screen.getAllByLabelText("URL");
      fireEvent.change(urlInputs[0], { target: { value: "https://new.example" } });
      // "Save" also names the properties card's own submit — the edit
      // row's is the last one on screen.
      const saveButtons = screen.getAllByRole("button", { name: "Save" });
      fireEvent.click(saveButtons[saveButtons.length - 1]);

      expect(onPatchProjectLink).toHaveBeenCalledWith(link, { url: "https://new.example", label: null });
    });

    it("swaps the positions of the two adjacent rows on a move gesture", () => {
      const onPatchProjectLink = vi.fn();
      const first = projectLinkDTO({ id: "l-1", position: 1 });
      const second = projectLinkDTO({ id: "l-2", position: 2 });
      openDossier({ linksByProject: { "p-1": [first, second] }, onPatchProjectLink });

      // Two rows, each carrying its own "Move down" — the first row's is
      // the enabled one (the last row's is disabled, nothing below it).
      fireEvent.click(screen.getAllByRole("button", { name: "Move down" })[0]);

      expect(onPatchProjectLink).toHaveBeenCalledWith(first, { position: 2 });
      expect(onPatchProjectLink).toHaveBeenCalledWith(second, { position: 1 });
    });

    it("flags removal with a timestamp rather than deleting from the store", () => {
      const onPatchProjectLink = vi.fn();
      const link = projectLinkDTO({ id: "l-1" });
      openDossier({ linksByProject: { "p-1": [link] }, onPatchProjectLink });

      fireEvent.click(screen.getByRole("button", { name: "Remove link" }));

      expect(onPatchProjectLink).toHaveBeenCalledWith(link, { removedAt: expect.any(Number) });
    });

    it("renders a failed link write's own message, scoped to this project", () => {
      // Must return the seed the simulated broadcast below carries — #669's
      // fix, ported from `ArchiveCard`'s own #668 fix: this card only reacts
      // to a `lastProjectLinkWrite` whose `seed` matches the one its own
      // write minted.
      const { task, rerenderWith } = openDossier({
        linksByProject: { "p-1": [] },
        onCreateProjectLink: () => "s-1",
      });
      fireEvent.change(screen.getByLabelText("URL"), { target: { value: "https://docs.example" } });
      fireEvent.click(screen.getByRole("button", { name: "Add link" }));

      rerenderWith({
        task: {
          ...task,
          lastProjectLinkWrite: { seed: "s-1", projectId: "p-1", kind: "failed", error: "url must be non-empty" },
        },
      });

      expect(screen.getByText("url must be non-empty")).toBeTruthy();
    });

    // #669's regression: under the old `projectId`-only gate, a foreign
    // write on the SAME project (a different seed) satisfied this card's
    // failure read just as readily as its own. Asserting on the rendered
    // label, not on internal state, is what makes this fail against the
    // pre-fix predicate.
    it("does not paint a foreign write's failure — same project, a different seed — into this card", () => {
      const { task, rerenderWith } = openDossier({
        linksByProject: { "p-1": [] },
        onCreateProjectLink: () => "s-1",
      });
      fireEvent.change(screen.getByLabelText("URL"), { target: { value: "https://docs.example" } });
      fireEvent.click(screen.getByRole("button", { name: "Add link" }));

      rerenderWith({
        task: {
          ...task,
          lastProjectLinkWrite: { seed: "foreign-seed", projectId: "p-1", kind: "failed", error: "no can do" },
        },
      });

      expect(screen.queryByText("no can do")).toBeNull();
    });

    it("does not paint another project's failed link write into this dossier", () => {
      openDossier({
        linksByProject: { "p-1": [] },
        lastProjectLinkWrite: { seed: "s-1", projectId: "p-2", kind: "failed", error: "no can do" },
      });

      expect(screen.queryByText("no can do")).toBeNull();
    });
  });

  // #628: the fog card.
  // The dossier's centre column: Now's board, filtered to the open project.
  // These are membership and chrome tests — the board's own behaviour
  // (optimistic fallback, the capture editor, collapse, facets) is
  // `NowScreen.test.tsx`'s, over the same component, and is deliberately not
  // re-asserted here.
  describe("the project board", () => {
    const inProject = itemDTO({
      id: "i-in",
      seq: 11,
      title: "Replace the fence panels",
      projectId: "p-1",
      context: "@errands",
    });
    const otherProject = itemDTO({
      id: "i-out",
      seq: 12,
      title: "Rake the leaves",
      projectId: "p-2",
      context: "@garden",
    });
    const unassigned = itemDTO({
      id: "i-none",
      seq: 13,
      title: "Book the dentist",
      projectId: null,
      context: "@phone",
    });

    function openBoard(
      task: ReturnType<typeof taskState>,
      overrides: Partial<ProjectsScreenProps> = {},
    ) {
      const rendered = renderProjectsScreen({ task, ...overrides });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));
      return rendered;
    }

    function twoProjects(overrides: Partial<ReturnType<typeof taskState>> = {}) {
      return taskState({
        projects: [
          projectDTO({ id: "p-1", name: "House repairs" }),
          projectDTO({ id: "p-2", name: "Autumn garden clear-up" }),
        ],
        ledger: [],
        ...overrides,
      });
    }

    it("shows this project's frontier items and no other project's", () => {
      openBoard(twoProjects({ frontier: [inProject, otherProject, unassigned] }));

      expect(screen.getByText("Replace the fence panels")).toBeTruthy();
      expect(screen.queryByText("Rake the leaves")).toBeNull();
      // Membership is `projectId` equality, so an item carrying no project
      // is not in this one either — "unassigned" is not "everywhere".
      expect(screen.queryByText("Book the dentist")).toBeNull();
    });

    it("includes an action assigned to the project but never positioned", () => {
      // The whole reason `project_pos` is not part of the predicate: the
      // action list this board replaced required it, so an item assigned by
      // triage and never dragged into order was invisible on this screen.
      openBoard(
        twoProjects({
          frontier: [itemDTO({ ...inProject, projectPos: null })],
        }),
      );

      expect(screen.getByText("Replace the fence panels")).toBeTruthy();
    });

    it("shows the project's own captures, and not another project's", () => {
      openBoard(
        twoProjects({
          triageInbox: [
            itemDTO({
              id: "c-in",
              seq: 21,
              title: "Sweeper: gutter quote",
              stage: "triage",
              projectId: "p-1",
            }),
            itemDTO({
              id: "c-out",
              seq: 22,
              title: "Sweeper: bulb order",
              stage: "triage",
              projectId: "p-2",
            }),
          ],
          grillingItems: [
            itemDTO({
              id: "g-in",
              seq: 23,
              title: "Still foggy: the deck permit",
              stage: "grilling",
              projectId: "p-1",
            }),
          ],
        }),
      );

      expect(screen.getByText("Sweeper: gutter quote")).toBeTruthy();
      expect(screen.getByText("Still foggy: the deck permit")).toBeTruthy();
      expect(screen.queryByText("Sweeper: bulb order")).toBeNull();
    });

    it("filters the Blocked section to this project", () => {
      openBoard(
        twoProjects({
          blocked: [
            blockedEntryDTO(
              itemDTO({ id: "b-in", seq: 31, title: "Fit the new gate", projectId: "p-1" }),
              [itemDTO({ id: "b-dep", seq: 32, title: "Await the survey" })],
            ),
            blockedEntryDTO(
              itemDTO({ id: "b-out", seq: 33, title: "Turn the compost", projectId: "p-2" }),
              [itemDTO({ id: "b-dep2", seq: 34, title: "Await the delivery" })],
            ),
          ],
        }),
      );

      expect(screen.getByText("Fit the new gate")).toBeTruthy();
      expect(screen.queryByText("Turn the compost")).toBeNull();
      // The section's own count is the filtered one, not the store's.
      expect(screen.getByText("1 action")).toBeTruthy();
    });

    it("offers every axis but Project, which would be one column here", () => {
      openBoard(twoProjects({ frontier: [inProject] }));

      expect(screen.getByRole("button", { name: "Context", pressed: true })).toBeTruthy();
      expect(screen.getByRole("button", { name: "Size" })).toBeTruthy();
      expect(screen.getByRole("button", { name: "Energy" })).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Project" })).toBeNull();
    });

    it("writes the axis under this screen's own keys, never Now's", () => {
      const storage = memoryStorage();
      openBoard(twoProjects({ frontier: [inProject] }), { storage });

      fireEvent.click(screen.getByRole("button", { name: "Size" }));

      expect(storage.entries["hb.projects.frontier-axis"]).toBe("size");
      expect("hb.now.frontier-axis" in storage.entries).toBe(false);
    });

    it("expands a selected in-project item above the columns", () => {
      const onCloseItemDetail = vi.fn();
      openBoard(twoProjects({ frontier: [inProject, otherProject] }), {
        selectedItemId: "i-in",
        onCloseItemDetail,
      });

      // The panel, not merely the card: its Close control is what only the
      // expanded slot renders.
      fireEvent.click(screen.getByRole("button", { name: "Close item detail" }));
      expect(onCloseItemDetail).toHaveBeenCalled();
    });

    it("renders no slot for a selection outside this project", () => {
      // Selection is app-global and deliberately not cleared on navigation:
      // an item selected on Now simply is not found here.
      openBoard(twoProjects({ frontier: [inProject, otherProject] }), {
        selectedItemId: "i-out",
      });

      expect(screen.queryByRole("button", { name: "Close item detail" })).toBeNull();
      expect(screen.getByText("Replace the fence panels")).toBeTruthy();
    });

    it("offers no Grill me on this surface", () => {
      // No `grill` prop is threaded here (`App.tsx`), so the takeover is
      // unreachable from a project rather than merely unused.
      openBoard(twoProjects({ frontier: [inProject] }), { selectedItemId: "i-in" });

      expect(screen.queryByRole("button", { name: /Grill me|Resume grill/ })).toBeNull();
    });

    it("keeps the aside's four record cards, and neither deleted card", () => {
      openBoard(twoProjects({ frontier: [inProject] }));

      const aside = screen.getByRole("complementary", { name: "Project properties" });
      expect(within(aside).getByLabelText("GitHub repo")).toBeTruthy();
      // In this order — Route on top, because it is the card a reader
      // consults while looking at the board beside it. Read off the DOM
      // rather than asserted card by card, so a re-ordering is a failure
      // and not four still-passing existence checks.
      expect(
        within(aside)
          .getAllByText(/^(route · destination|properties|links|archive)$/)
          .map((node) => node.textContent),
      ).toEqual(["route · destination", "properties", "links", "archive"]);
      // Both deleted cards, gone rather than merely unwired.
      expect(screen.queryByText(/fog/i)).toBeNull();
      expect(screen.queryByText("actions")).toBeNull();
    });
  });

  describe("the route card", () => {
    function openDossier(
      overrides: {
        routeByProject?: ReturnType<typeof taskState>["routeByProject"];
        lastRouteWrite?: ReturnType<typeof taskState>["lastRouteWrite"];
        onRequestRoute?: ProjectsScreenProps["onRequestRoute"];
        onPatchRoute?: ProjectsScreenProps["onPatchRoute"];
      } = {},
    ) {
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        routeByProject: overrides.routeByProject ?? {},
        lastRouteWrite: overrides.lastRouteWrite ?? null,
      });
      const { rerenderWith } = renderProjectsScreen({
        task,
        onRequestRoute: overrides.onRequestRoute ?? noop,
        onPatchRoute: overrides.onPatchRoute ?? noopWriteSeed,
      });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));
      return { task, rerenderWith };
    }

    // The Route card and the properties card both carry a "Save" button —
    // the Route card's is first in document order (the reading column
    // renders before the aside), so this is always index 0.
    function routeSaveButton(): HTMLElement {
      return screen.getAllByRole("button", { name: "Save" })[0];
    }

    it("requests this project's Route the moment the dossier opens", () => {
      const onRequestRoute = vi.fn();
      openDossier({ onRequestRoute });

      expect(onRequestRoute).toHaveBeenCalledWith("p-1");
    });

    it("holds rather than claiming anything while the read has not answered", () => {
      openDossier();

      expect(screen.getByText("Reading Route…")).toBeTruthy();
      expect(screen.queryByLabelText("Destination")).toBeNull();
    });

    it("renders the stored destination and notes once read", () => {
      openDossier({
        routeByProject: {
          "p-1": routeDTO({ destination: "Ship the deck", notes: "Ask the neighbour" }),
        },
      });

      expect((screen.getByLabelText("Destination") as HTMLTextAreaElement).value).toBe("Ship the deck");
      expect((screen.getByLabelText("Notes") as HTMLTextAreaElement).value).toBe("Ask the neighbour");
    });

    it("sends only the changed field to onPatchRoute", () => {
      const onPatchRoute = vi.fn();
      const route = routeDTO({ projectId: "p-1" });
      openDossier({ routeByProject: { "p-1": route }, onPatchRoute });

      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });
      fireEvent.click(routeSaveButton());

      expect(onPatchRoute).toHaveBeenCalledWith(route, { destination: "Ship the deck" });
    });

    it("clears a field by saving it empty", () => {
      const onPatchRoute = vi.fn();
      const route = routeDTO({ projectId: "p-1", notes: "Ask the neighbour" });
      openDossier({ routeByProject: { "p-1": route }, onPatchRoute });

      fireEvent.change(screen.getByLabelText("Notes"), { target: { value: "" } });
      fireEvent.click(routeSaveButton());

      expect(onPatchRoute).toHaveBeenCalledWith(route, { notes: null });
    });

    it("disables Save until a field actually changes", () => {
      openDossier({ routeByProject: { "p-1": routeDTO({ projectId: "p-1" }) } });

      expect(routeSaveButton().hasAttribute("disabled")).toBe(true);

      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });

      expect(routeSaveButton().hasAttribute("disabled")).toBe(false);
    });

    // The card has no optimistic overlay (`Core::patch_route`'s own doc), so
    // this is its whole visible consequence: "Saving…" from the click until
    // the row's version actually moves, then "Saved" — the explicit state
    // this slice's brief asks for, unlike the properties card's silent
    // re-sync.
    it("says Saving… after Save, then Saved once the row's version lands", () => {
      const route = routeDTO({ projectId: "p-1", version: 1 });
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        routeByProject: { "p-1": route },
      });
      const { rerenderWith } = renderProjectsScreen({ task, onPatchRoute: noopWriteSeed });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });
      fireEvent.click(routeSaveButton());

      expect(screen.getByText("Saving…")).toBeTruthy();

      const landed = { ...route, destination: "Ship the deck", version: 2 };
      rerenderWith({ task: { ...task, routeByProject: { "p-1": landed } } });

      expect(screen.queryByText("Saving…")).toBeNull();
      expect(screen.getByText("Saved")).toBeTruthy();
    });

    // Regression: a write that never reaches the queue (a durability
    // failure, or the wasm host busy mid sync-cycle at click time) never
    // produces a version bump, so the only other resolution path
    // (`route.version !== synced.version`) never fires either. `saving`
    // must clear on the write result itself, or "Saving…" renders forever
    // beside the danger badge painted from the same `lastRouteWrite` —
    // contradictory UI.
    it.each([
      ["failed", "no can do"],
      ["busy", null],
    ] as const)("clears Saving… when the write result is %s, not stuck beside the failure badge", (kind, error) => {
      const route = routeDTO({ projectId: "p-1", version: 1 });
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        routeByProject: { "p-1": route },
      });
      // Must return the seed the simulated broadcast below carries — the
      // same seed-keyed contract every other write result on this screen
      // now carries (#669).
      const { rerenderWith } = renderProjectsScreen({ task, onPatchRoute: () => "s-1" });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });
      fireEvent.click(routeSaveButton());

      expect(screen.getByText("Saving…")).toBeTruthy();

      rerenderWith({
        task: {
          ...task,
          lastRouteWrite: { seed: "s-1", projectId: "p-1", kind, error },
        },
      });

      expect(screen.queryByText("Saving…")).toBeNull();
      expect(screen.queryByText("Saved")).toBeNull();
      expect(screen.getByText(error ?? "That route write did not go through.")).toBeTruthy();
    });

    it("keeps a field typed while another field's write was in flight", () => {
      // Both fields share one Save and one version, so the destination's
      // write landing moves the version while notes is half-typed. The
      // re-seed must skip the field being edited, or the typing is lost —
      // `PropertiesCard`'s own guarantee, ported.
      const route = routeDTO({ projectId: "p-1" });
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        routeByProject: { "p-1": route },
      });
      const { rerenderWith } = renderProjectsScreen({ task, onPatchRoute: noopWriteSeed });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });
      fireEvent.click(routeSaveButton());
      fireEvent.change(screen.getByLabelText("Notes"), { target: { value: "Ask the neighbour" } });

      const landed = { ...route, destination: "Ship the deck", version: 2 };
      rerenderWith({ task: { ...task, routeByProject: { "p-1": landed } } });

      expect((screen.getByLabelText("Notes") as HTMLTextAreaElement).value).toBe("Ask the neighbour");
      expect((screen.getByLabelText("Destination") as HTMLTextAreaElement).value).toBe("Ship the deck");
    });

    it("renders a failed route write's own message, scoped to this project", () => {
      // Must return the seed the simulated broadcast below carries — same
      // seed-keyed contract `ArchiveCard`'s #668 fix established, ported to
      // this card by #669.
      const { task, rerenderWith } = openDossier({
        routeByProject: { "p-1": routeDTO({ projectId: "p-1" }) },
        onPatchRoute: () => "s-1",
      });
      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });
      fireEvent.click(routeSaveButton());

      rerenderWith({
        task: { ...task, lastRouteWrite: { seed: "s-1", projectId: "p-1", kind: "failed", error: "no can do" } },
      });

      expect(screen.getByText("no can do")).toBeTruthy();
    });

    it("does not paint another project's failed route write into this dossier", () => {
      openDossier({
        routeByProject: { "p-1": routeDTO({ projectId: "p-1" }) },
        lastRouteWrite: { seed: "s-1", projectId: "p-2", kind: "failed", error: "no can do" },
      });

      expect(screen.queryByText("no can do")).toBeNull();
    });

    // #669's regression: under the old `projectId`-only gate, a foreign
    // write on the SAME project (a different seed — e.g. a sibling card's
    // own write, or a stale result from an earlier save this card already
    // resolved) satisfied this card's failure read just as readily as its
    // own, suppressing "Saving…" over a write that is still queued.
    // Asserting on the rendered label, not on internal state, is what makes
    // this fail against the pre-fix predicate.
    it("keeps Saving… through a foreign write's failure — same project, a different seed", () => {
      const route = routeDTO({ projectId: "p-1", version: 1 });
      const task = taskState({
        projects: [projectDTO({ id: "p-1", name: "House repairs" })],
        ledger: [],
        routeByProject: { "p-1": route },
      });
      const { rerenderWith } = renderProjectsScreen({ task, onPatchRoute: () => "s-1" });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

      fireEvent.change(screen.getByLabelText("Destination"), { target: { value: "Ship the deck" } });
      fireEvent.click(routeSaveButton());

      expect(screen.getByText("Saving…")).toBeTruthy();

      rerenderWith({
        task: {
          ...task,
          lastRouteWrite: { seed: "foreign-seed", projectId: "p-1", kind: "failed", error: "no can do" },
        },
      });

      expect(screen.getByText("Saving…")).toBeTruthy();
      expect(screen.queryByText("no can do")).toBeNull();
    });
  });
});
