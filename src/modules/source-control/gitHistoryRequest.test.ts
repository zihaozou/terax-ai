import { describe, expect, it } from "vitest";
import { createGitHistoryRequestGate } from "./gitHistoryRequest";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

describe("Git history request ownership", () => {
  it("does not open an A request after switching to Space B", async () => {
    const gate = createGitHistoryRequestGate();
    const pending = deferred<{ repoRoot: string }>();
    const opened: string[] = [];
    const request = gate.begin("space-a", "local");
    const complete = pending.promise.then((repo) => {
      if (gate.isCurrent(request, "space-b", "local")) {
        opened.push(repo.repoRoot);
      }
    });

    pending.resolve({ repoRoot: "/repos/a" });
    await complete;

    expect(opened).toEqual([]);
  });

  it("allows only the latest request to open", async () => {
    const gate = createGitHistoryRequestGate();
    const first = deferred<{ repoRoot: string }>();
    const second = deferred<{ repoRoot: string }>();
    const opened: string[] = [];
    const firstRequest = gate.begin("space-a", "local");
    const secondRequest = gate.begin("space-a", "local");
    const complete = (
      request: typeof firstRequest,
      pending: Promise<{ repoRoot: string }>,
    ) =>
      pending.then((repo) => {
        if (gate.isCurrent(request, "space-a", "local")) {
          opened.push(repo.repoRoot);
        }
      });

    const firstCompletion = complete(firstRequest, first.promise);
    const secondCompletion = complete(secondRequest, second.promise);
    second.resolve({ repoRoot: "/repos/newer" });
    await secondCompletion;
    first.resolve({ repoRoot: "/repos/older" });
    await firstCompletion;

    expect(opened).toEqual(["/repos/newer"]);
  });
});
