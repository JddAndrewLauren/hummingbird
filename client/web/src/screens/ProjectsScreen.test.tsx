// @vitest-environment jsdom
//
// The gate for #624's screen. `client/web` has a documented trap — it can ship
// UI state with no reader, and `tsc` cannot see a missing caller — so what
// these assert is that the screen renders **from `TaskState`**, not that its
// pieces exist.
import { describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  ledgerRowDTO,
  projectDTO,
  projectLinkDTO,
  render,
  screen,
  taskState,
} from "../test/component";
import { ProjectsScreen, type ProjectsScreenProps } from "./ProjectsScreen";

const noop = () => {};

/** #626 added three more required props (the links card's read/write doors)
 * — every call site here cares only about the
 * project create/patch behaviour this file already covers, so this helper
 * supplies the new ones' defaults once rather than repeating them at every
 * `render` call. The refetch key rides `task.syncOutcomeSeq`, not a prop. */
function renderProjectsScreen(props: Partial<ProjectsScreenProps> & Pick<ProjectsScreenProps, "task">) {
  return render(
    <ProjectsScreen
      onCreateProject={noop}
      onPatchProject={noop}
      onRequestProjectLinks={noop}
      onCreateProjectLink={noop}
      onPatchProjectLink={noop}
      {...props}
    />,
  );
}

describe("ProjectsScreen", () => {
  it("holds rather than claiming 'no projects' while the read has not answered", () => {
    renderProjectsScreen({ task: taskState(), onCreateProject: noop, onPatchProject: noop });

    expect(screen.getByText("Reading projects…")).toBeTruthy();
    expect(screen.queryByText("No projects yet")).toBeNull();
  });

  it("renders the empty state only for a real, empty answer", () => {
    renderProjectsScreen({ task: taskState({ projects: [] }), onCreateProject: noop, onPatchProject: noop });

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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

    expect(screen.getByText("1 live · 1 archived")).toBeTruthy();
    expect(screen.queryByRole("heading", { level: 3, name: "Old bike" })).toBeNull();

    fireEvent.click(screen.getByRole("switch", { name: "Show archived" }));

    expect(screen.getByRole("heading", { level: 3, name: "Old bike" })).toBeTruthy();
    expect(screen.getByText("archived")).toBeTruthy();
  });

  it("sends the trimmed name to onCreateProject and refuses a blank one", () => {
    const onCreateProject = vi.fn();
    renderProjectsScreen({ task: taskState({ projects: [], ledger: [] }), onCreateProject: onCreateProject, onPatchProject: noop });

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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

    expect(screen.getByText("creating — appears when the round trip lands")).toBeTruthy();
  });

  it("drops the waiting line once the project reaches the grid", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-new", name: "Rebuild the deck" })],
      ledger: [],
      lastProjectWrite: { seed: "s-1", projectId: "p-new", kind: "ok", error: null },
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

    expect(screen.getByText("That project write did not go through.")).toBeTruthy();
    expect(screen.queryByText("creating — appears when the round trip lands")).toBeNull();
  });

  it("opens a project to the dossier shell and comes back to the grid", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.getByRole("heading", { level: 2, name: "House repairs" })).toBeTruthy();
    // The unbuilt regions are labelled rather than absent, so an operator
    // meets a region that is coming, not one that is broken.
    expect(screen.getByText("route · destination")).toBeTruthy();
    expect(screen.getByText("The open questions on this Route land here.")).toBeTruthy();

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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    const link = screen.getByRole("link", { name: "https://github.com/JddAndrewLauren/hummingbird" });
    expect(link.getAttribute("href")).toBe("https://github.com/JddAndrewLauren/hummingbird");
    expect((screen.getByLabelText("GitHub repo") as HTMLInputElement).value).toBe(
      "JddAndrewLauren/hummingbird",
    );
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
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.queryByText("no can do")).toBeNull();
  });

  it("disables Save until a field actually changes", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    renderProjectsScreen({ task: task, onCreateProject: noop, onPatchProject: noop });
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(true);

    fireEvent.change(screen.getByLabelText("GitHub repo"), { target: { value: "a/b" } });

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(false);
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
      renderProjectsScreen({
        task,
        onRequestProjectLinks: overrides.onRequestProjectLinks ?? noop,
        onCreateProjectLink: overrides.onCreateProjectLink ?? noop,
        onPatchProjectLink: overrides.onPatchProjectLink ?? noop,
      });
      fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));
      return task;
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
          onPatchProject={noop}
          onRequestProjectLinks={onRequestProjectLinks}
          onCreateProjectLink={noop}
          onPatchProjectLink={noop}
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
      openDossier({
        linksByProject: { "p-1": [] },
        lastProjectLinkWrite: { seed: "s-1", projectId: "p-1", kind: "failed", error: "url must be non-empty" },
      });

      expect(screen.getByText("url must be non-empty")).toBeTruthy();
    });

    it("does not paint another project's failed link write into this dossier", () => {
      openDossier({
        linksByProject: { "p-1": [] },
        lastProjectLinkWrite: { seed: "s-1", projectId: "p-2", kind: "failed", error: "no can do" },
      });

      expect(screen.queryByText("no can do")).toBeNull();
    });
  });
});
