// @vitest-environment jsdom
//
// The gate for #624's screen. `client/web` has a documented trap — it can ship
// UI state with no reader, and `tsc` cannot see a missing caller — so what
// these assert is that the screen renders **from `TaskState`**, not that its
// pieces exist.
import { describe, expect, it, vi } from "vitest";
import { fireEvent, ledgerRowDTO, projectDTO, render, screen, taskState } from "../test/component";
import { ProjectsScreen } from "./ProjectsScreen";

const noop = () => {};

describe("ProjectsScreen", () => {
  it("holds rather than claiming 'no projects' while the read has not answered", () => {
    render(<ProjectsScreen task={taskState()} onCreateProject={noop} onPatchProject={noop} />);

    expect(screen.getByText("Reading projects…")).toBeTruthy();
    expect(screen.queryByText("No projects yet")).toBeNull();
  });

  it("renders the empty state only for a real, empty answer", () => {
    render(<ProjectsScreen task={taskState({ projects: [] })} onCreateProject={noop} onPatchProject={noop} />);

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

    expect(screen.getByText("1 live · 1 archived")).toBeTruthy();
    expect(screen.queryByRole("heading", { level: 3, name: "Old bike" })).toBeNull();

    fireEvent.click(screen.getByRole("switch", { name: "Show archived" }));

    expect(screen.getByRole("heading", { level: 3, name: "Old bike" })).toBeTruthy();
    expect(screen.getByText("archived")).toBeTruthy();
  });

  it("sends the trimmed name to onCreateProject and refuses a blank one", () => {
    const onCreateProject = vi.fn();
    render(
      <ProjectsScreen task={taskState({ projects: [], ledger: [] })} onCreateProject={onCreateProject} onPatchProject={noop} />,
    );

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

    expect(screen.getByText("creating — appears when the round trip lands")).toBeTruthy();
  });

  it("drops the waiting line once the project reaches the grid", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-new", name: "Rebuild the deck" })],
      ledger: [],
      lastProjectWrite: { seed: "s-1", projectId: "p-new", kind: "ok", error: null },
    });
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

    expect(screen.getByText("That project write did not go through.")).toBeTruthy();
    expect(screen.queryByText("creating — appears when the round trip lands")).toBeNull();
  });

  it("opens a project to the dossier shell and comes back to the grid", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={onPatchProject} />);
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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={onPatchProject} />);
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
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);

    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.queryByText("no can do")).toBeNull();
  });

  it("disables Save until a field actually changes", () => {
    const task = taskState({
      projects: [projectDTO({ id: "p-1", name: "House repairs" })],
      ledger: [],
    });
    render(<ProjectsScreen task={task} onCreateProject={noop} onPatchProject={noop} />);
    fireEvent.click(screen.getByRole("heading", { level: 3, name: "House repairs" }));

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(true);

    fireEvent.change(screen.getByLabelText("GitHub repo"), { target: { value: "a/b" } });

    expect(screen.getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(false);
  });
});
