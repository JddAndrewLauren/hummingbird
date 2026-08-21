import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkerLike } from "../store/worker-client";
import { projectDTO, projectLinkDTO, routeDTO } from "../test/component";
import {
  mintProjectPatchSeed,
  useProjectsWiring,
} from "./useProjectsWiring";

// #672: `useProjectsWiring.ts` had no test file of its own, and since #668/
// #669 it owns the seed contract `ArchiveCard`'s pending gate depends on —
// `patchProject` (and every sibling write here) must RETURN the seed it
// mints, because `lastProjectWrite`/`lastProjectLinkWrite`/`lastRouteWrite`
// are each one broadcast slot shared by every card mounted on the same
// dossier. A regression that dropped the return would typecheck clean (the
// prop type callers declare is `=> void`, which `=> string` satisfies) and
// only a test that reads the return value catches it.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("mintProjectPatchSeed", () => {
  it("retrying the same patch intent (same project, same nowMs) mints the same seed", () => {
    const first = mintProjectPatchSeed("project-1", 5_000);
    const second = mintProjectPatchSeed("project-1", 5_000);

    expect(first).toEqual(second);
  });

  it("a different nowMs mints a distinct seed for the same project", () => {
    const first = mintProjectPatchSeed("project-1", 5_000);
    const second = mintProjectPatchSeed("project-1", 5_001);

    expect(first).not.toEqual(second);
  });

  it("a different project mints a distinct seed at the same nowMs", () => {
    const first = mintProjectPatchSeed("project-1", 5_000);
    const second = mintProjectPatchSeed("project-2", 5_000);

    expect(first).not.toEqual(second);
  });

  // Deliberate, per the hook's own doc: this is a known consequence of
  // determinism, not a bug to "fix" into a random id.
  it("two patches to the same project in the same millisecond mint identical seeds", () => {
    const first = mintProjectPatchSeed("project-1", 5_000);
    const second = mintProjectPatchSeed("project-1", 5_000);

    expect(first).toEqual(second);
  });
});

describe("useProjectsWiring: createProject", () => {
  it("posts a createProject message and returns the seed it minted", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);

    const seed = wiring.createProject("A new project");

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "createProject",
      seed,
      name: "A new project",
      nowMs: 1_000,
    });
  });

  it("two creates in the same millisecond mint distinct seeds — a new entity each time", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);

    const first = wiring.createProject("First");
    const second = wiring.createProject("Second");

    expect(first).not.toEqual(second);
  });
});

describe("useProjectsWiring: patchProject", () => {
  it("returns the seed it mints, deterministic on the project id and nowMs", () => {
    vi.useFakeTimers();
    vi.setSystemTime(5_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);
    const current = projectDTO({ id: "project-1" });

    const seed = wiring.patchProject(current, { githubRepo: "org/repo" });

    expect(seed).toEqual(mintProjectPatchSeed("project-1", 5_000));
  });

  it("the seed it returns is the same seed handed to the worker door", () => {
    vi.useFakeTimers();
    vi.setSystemTime(5_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);
    const current = projectDTO({ id: "project-1" });

    const seed = wiring.patchProject(current, { githubRepo: "org/repo" });

    const message = worker.postMessage.mock.calls[0][0] as { seed: string };
    expect(message.seed).toEqual(seed);
  });

  it("posts a patchProject message with only the changed field marked touched", () => {
    vi.useFakeTimers();
    vi.setSystemTime(5_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);
    const current = projectDTO({ id: "project-1" });

    wiring.patchProject(current, { archivedAt: 9_000 });

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "patchProject",
      seed: mintProjectPatchSeed("project-1", 5_000),
      current,
      name: null,
      githubRepoTouched: false,
      githubRepo: null,
      defaultContextTouched: false,
      defaultContext: null,
      archivedAtTouched: true,
      archivedAt: 9_000,
      nowMs: 5_000,
    });
  });
});

describe("useProjectsWiring: createProjectLink", () => {
  it("posts a createProjectLink message and returns the seed it minted", () => {
    vi.useFakeTimers();
    vi.setSystemTime(2_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);

    const seed = wiring.createProjectLink("project-1", "https://example.com", "Example", 1);

    expect(worker.postMessage).toHaveBeenCalledWith({
      type: "createProjectLink",
      seed,
      projectId: "project-1",
      url: "https://example.com",
      label: "Example",
      position: 1,
      nowMs: 2_000,
    });
  });
});

describe("useProjectsWiring: patchProjectLink", () => {
  it("returns the seed it mints, and hands the same seed to the worker door", () => {
    vi.useFakeTimers();
    vi.setSystemTime(3_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);
    const current = projectLinkDTO({ id: "link-1" });

    const seed = wiring.patchProjectLink(current, { url: "https://changed.example.com" });

    const message = worker.postMessage.mock.calls[0][0] as { type: string; seed: string };
    expect(message.type).toEqual("patchProjectLink");
    expect(message.seed).toEqual(seed);
  });
});

describe("useProjectsWiring: patchRoute", () => {
  it("returns the seed it mints, and hands the same seed to the worker door", () => {
    vi.useFakeTimers();
    vi.setSystemTime(4_000);
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);
    const current = routeDTO({ projectId: "project-1" });

    const seed = wiring.patchRoute(current, { destination: "somewhere" });

    const message = worker.postMessage.mock.calls[0][0] as { type: string; seed: string };
    expect(message.type).toEqual("patchRoute");
    expect(message.seed).toEqual(seed);
  });
});

describe("useProjectsWiring: read doors", () => {
  it("requestProjectLinks and requestRoute post their per-id fetch, no seed", () => {
    const worker = fakeWorker();
    const wiring = useProjectsWiring(worker);

    wiring.requestProjectLinks("project-1");
    wiring.requestRoute("project-1");

    expect(worker.postMessage).toHaveBeenNthCalledWith(1, {
      type: "getProjectLinks",
      projectId: "project-1",
    });
    expect(worker.postMessage).toHaveBeenNthCalledWith(2, {
      type: "getRoute",
      projectId: "project-1",
    });
  });
});
